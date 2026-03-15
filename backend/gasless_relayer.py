import os
import time
import logging
import asyncio
import hashlib
import hmac
from collections import defaultdict
from fastapi import FastAPI, HTTPException, Request, Depends
from pydantic import BaseModel
from web3 import Web3
from web3.exceptions import ContractLogicError

logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
logger = logging.getLogger("0-ads-Relayer")

app = FastAPI(title="0-ads Gasless Relayer Node")

# --- Configuration ---
RPC_URL = os.environ.get("RPC_URL", "https://sepolia.base.org")
CHAIN_ID = int(os.environ.get("CHAIN_ID", 84532))
RELAYER_PRIVATE_KEY = os.environ.get("RELAYER_PRIVATE_KEY")
CONTRACT_ADDRESS = os.environ.get("CONTRACT_ADDRESS", "0x8a2aD6bC4A240515c49035bE280BacB7CA94afC4")
RELAYER_API_KEYS: set = set(filter(None, os.environ.get("RELAYER_API_KEYS", "").split(",")))
RATE_LIMIT_PER_MIN: int = int(os.environ.get("RELAYER_RATE_LIMIT_RPM", "30"))

if not RELAYER_PRIVATE_KEY:
    logger.warning("RELAYER_PRIVATE_KEY is missing! Relayer will start in dry-run mode.")

w3 = Web3(Web3.HTTPProvider(RPC_URL))
try:
    relayer_account = w3.eth.account.from_key(RELAYER_PRIVATE_KEY) if RELAYER_PRIVATE_KEY else None
    relayer_address = relayer_account.address if relayer_account else "DRY_RUN_MODE"
    logger.info(f"Relayer initialized. Operating Wallet: {relayer_address}")
except Exception as e:
    logger.error(f"Failed to initialize Relayer Wallet: {e}")
    relayer_account = None

ABI = [{
    "inputs": [
        {"internalType": "bytes32", "name": "campaignId", "type": "bytes32"},
        {"internalType": "address", "name": "agent", "type": "address"},
        {"internalType": "uint256", "name": "deadline", "type": "uint256"},
        {"internalType": "bytes", "name": "oracleSignature", "type": "bytes"}
    ],
    "name": "claimPayoutFor",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
}]


# ---------------------------------------------------------------------------
# Nonce Manager — serializes tx nonce assignment under an asyncio lock
# ---------------------------------------------------------------------------

class NonceManager:
    def __init__(self, w3_instance: Web3, address: str):
        self._w3 = w3_instance
        self._address = address
        self._lock = asyncio.Lock()
        self._current_nonce: int | None = None

    async def get_next_nonce(self) -> int:
        async with self._lock:
            on_chain = self._w3.eth.get_transaction_count(self._address, "pending")
            if self._current_nonce is None or on_chain > self._current_nonce:
                self._current_nonce = on_chain
            nonce = self._current_nonce
            self._current_nonce += 1
            return nonce

    async def reset(self):
        async with self._lock:
            self._current_nonce = None


nonce_manager: NonceManager | None = None
if relayer_account:
    nonce_manager = NonceManager(w3, relayer_address)


# ---------------------------------------------------------------------------
# IP-based rate limiter (sliding window)
# ---------------------------------------------------------------------------

class IPRateLimiter:
    def __init__(self, max_per_min: int):
        self._max = max_per_min
        self._windows: dict[str, list[float]] = defaultdict(list)

    def allow(self, ip: str) -> bool:
        now = time.monotonic()
        window = self._windows[ip]
        self._windows[ip] = [t for t in window if now - t < 60]
        if len(self._windows[ip]) >= self._max:
            return False
        self._windows[ip].append(now)
        return True


rate_limiter = IPRateLimiter(RATE_LIMIT_PER_MIN)


# ---------------------------------------------------------------------------
# Auth dependency
# ---------------------------------------------------------------------------

def _client_ip(request: Request) -> str:
    return request.client.host if request.client else "unknown"


async def verify_api_key(request: Request):
    if not RELAYER_API_KEYS:
        return
    key = request.headers.get("x-api-key", "")
    if key not in RELAYER_API_KEYS:
        raise HTTPException(status_code=401, detail="Invalid or missing x-api-key")


# ---------------------------------------------------------------------------
# Relay endpoint
# ---------------------------------------------------------------------------

class RelayerPayload(BaseModel):
    campaign_id: str
    deadline: int
    oracle_signature: str
    recipient: str


@app.post("/api/v1/relayer/execute", dependencies=[Depends(verify_api_key)])
async def execute_relay(payload: RelayerPayload, request: Request):
    if not relayer_account or not nonce_manager:
        raise HTTPException(status_code=503, detail="Relayer wallet not configured.")

    client_ip = _client_ip(request)
    if not rate_limiter.allow(client_ip):
        raise HTTPException(status_code=429, detail="Rate limit exceeded. Try again later.")

    logger.info(f"Relay request from {client_ip} for agent {payload.recipient} on campaign {payload.campaign_id}")

    contract = w3.eth.contract(address=CONTRACT_ADDRESS, abi=ABI)

    try:
        c_id_bytes = bytes.fromhex(payload.campaign_id.replace("0x", ""))
        if len(c_id_bytes) != 32:
            raise ValueError("campaign_id must be 32 bytes")
        sig_bytes = bytes.fromhex(payload.oracle_signature.replace("0x", ""))
        if len(sig_bytes) != 65:
            raise ValueError("oracle_signature must be 65 bytes")
        agent_addr = Web3.to_checksum_address(payload.recipient)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=f"Invalid hex formatting: {e}")

    try:
        tx_func = contract.functions.claimPayoutFor(c_id_bytes, agent_addr, payload.deadline, sig_bytes)
        estimated_gas = tx_func.estimate_gas({'from': relayer_address})
        logger.info(f"Simulation passed. Estimated Gas: {estimated_gas}")
    except ContractLogicError as cle:
        logger.warning(f"Simulation Reverted! Rejecting payload. Reason: {cle}")
        raise HTTPException(status_code=422, detail=f"Transaction would revert: {cle}")
    except Exception as e:
        logger.error(f"Simulation failed: {e}")
        raise HTTPException(status_code=422, detail=f"Simulation failed: {e}")

    try:
        gas_limit = int(estimated_gas * 1.2)
        nonce = await nonce_manager.get_next_nonce()

        tx = tx_func.build_transaction({
            'from': relayer_address,
            'nonce': nonce,
            'gas': gas_limit,
            'gasPrice': w3.eth.gas_price,
            'chainId': CHAIN_ID
        })

        signed_tx = w3.eth.account.sign_transaction(tx, private_key=RELAYER_PRIVATE_KEY)
        tx_hash = w3.eth.send_raw_transaction(signed_tx.raw_transaction)

        logger.info(f"Transaction Dispatched! Hash: {tx_hash.hex()}")
        return {"status": "ok", "tx_hash": tx_hash.hex()}

    except Exception as e:
        logger.error(f"Failed to broadcast transaction: {e}")
        raise HTTPException(status_code=500, detail=f"Failed to broadcast: {e}")


@app.get("/health")
async def health():
    return {"status": "ok", "wallet": relayer_address}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8081)
