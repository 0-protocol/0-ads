import os
import json
import logging
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from web3 import Web3
from web3.exceptions import ContractLogicError

# Setup Logging
logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
logger = logging.getLogger("0-ads-Relayer")

app = FastAPI(title="0-ads Gasless Relayer Node")

# --- Configuration ---
RPC_URL = os.environ.get("RPC_URL", "https://sepolia.base.org")
CHAIN_ID = int(os.environ.get("CHAIN_ID", 84532))
RELAYER_PRIVATE_KEY = os.environ.get("RELAYER_PRIVATE_KEY")
CONTRACT_ADDRESS = os.environ.get("CONTRACT_ADDRESS", "0x8a2aD6bC4A240515c49035bE280BacB7CA94afC4")

if not RELAYER_PRIVATE_KEY:
    logger.warning("⚠️ RELAYER_PRIVATE_KEY is missing! Relayer will start in dry-run mode or fail transactions.")

w3 = Web3(Web3.HTTPProvider(RPC_URL))
try:
    relayer_account = w3.eth.account.from_key(RELAYER_PRIVATE_KEY) if RELAYER_PRIVATE_KEY else None
    relayer_address = relayer_account.address if relayer_account else "DRY_RUN_MODE"
    logger.info(f"Relayer initialized. Operating Wallet: {relayer_address}")
except Exception as e:
    logger.error(f"Failed to initialize Relayer Wallet: {e}")
    relayer_account = None

# Minimal ABI for AdEscrow
ABI = [{
    "inputs": [
        {"internalType": "bytes32", "name": "campaignId", "type": "bytes32"},
        {"internalType": "uint256", "name": "deadline", "type": "uint256"},
        {"internalType": "bytes", "name": "oracleSignature", "type": "bytes"}
    ],
    "name": "claimPayout",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
}]

class RelayerPayload(BaseModel):
    campaign_id: str
    deadline: int
    oracle_signature: str
    recipient: str  # Kept for logging/accounting purposes

@app.post("/api/v1/relayer/execute")
async def execute_relay(payload: RelayerPayload):
    if not relayer_account:
        raise HTTPException(status_code=503, detail="Relayer wallet not configured.")
        
    logger.info(f"Received relay request for Agent {payload.recipient} on Campaign {payload.campaign_id}")
    
    contract = w3.eth.contract(address=CONTRACT_ADDRESS, abi=ABI)
    
    # 1. Format inputs
    try:
        c_id_bytes = bytes.fromhex(payload.campaign_id.replace("0x", ""))
        sig_bytes = bytes.fromhex(payload.oracle_signature.replace("0x", ""))
    except ValueError as e:
        raise HTTPException(status_code=400, detail=f"Invalid hex formatting: {e}")

    # 2. Simulate Transaction (Crucial Anti-DDoS / Gas Drain Protection)
    # If the oracle signature is invalid or the agent already claimed, this will revert!
    try:
        tx_func = contract.functions.claimPayout(c_id_bytes, payload.deadline, sig_bytes)
        estimated_gas = tx_func.estimate_gas({'from': relayer_address})
        logger.info(f"Simulation passed. Estimated Gas: {estimated_gas}")
    except ContractLogicError as cle:
        logger.warning(f"Simulation Reverted! Rejecting payload. Reason: {cle}")
        return {"error": f"Transaction would revert: {cle}"}
    except Exception as e:
        logger.error(f"Simulation failed: {e}")
        return {"error": f"Simulation failed: {e}"}

    # 3. Build & Dispatch Transaction
    try:
        # We add a 20% buffer to the estimated gas to ensure execution
        gas_limit = int(estimated_gas * 1.2)
        
        tx = tx_func.build_transaction({
            'from': relayer_address,
            'nonce': w3.eth.get_transaction_count(relayer_address),
            'gas': gas_limit,
            'gasPrice': w3.eth.gas_price,
            'chainId': CHAIN_ID
        })
        
        signed_tx = w3.eth.account.sign_transaction(tx, private_key=RELAYER_PRIVATE_KEY)
        tx_hash = w3.eth.send_raw_transaction(signed_tx.raw_transaction)
        
        logger.info(f"🚀 Transaction Dispatched! Hash: {tx_hash.hex()}")
        return {"status": "ok", "tx_hash": tx_hash.hex()}
        
    except Exception as e:
        logger.error(f"Failed to broadcast transaction: {e}")
        return {"error": f"Failed to broadcast: {e}"}

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8081)
