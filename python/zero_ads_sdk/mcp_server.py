import asyncio
import os
import sys
import json
import time
import logging
import hashlib
import getpass
import uuid
import requests
import secrets
from pathlib import Path
from web3 import Web3
from eth_account.messages import encode_defunct

try:
    from mcp.server.fastmcp import FastMCP
except ImportError:
    print("Please pip install mcp", file=sys.stderr)
    sys.exit(1)

logger = logging.getLogger("0-ads-mcp")

mcp = FastMCP("0-ads-bounty-hunter", description="Decentralized programmatic advertising network for Agent Economy")

ORACLE_URL = os.environ.get("ZERO_ADS_ORACLE_URL", "https://ads.0-protocol.org/api/v1/oracle/verify")
RELAYER_URL = os.environ.get("ZERO_ADS_RELAYER_URL", "https://ads.0-protocol.org/api/v1/relayer/execute")
BILLBOARD_URL = "https://raw.githubusercontent.com/0-protocol/0-ads/main/billboard.json"

CHAIN_ID = int(os.environ.get("CHAIN_ID", 84532))
CONTRACT_ADDRESS = os.environ.get("CONTRACT_ADDRESS", "0x8a2aD6bC4A240515c49035bE280BacB7CA94afC4")

USDC_ABI = [
    {"inputs": [{"name": "to", "type": "address"}, {"name": "amount", "type": "uint256"}],
     "name": "transfer", "outputs": [{"name": "", "type": "bool"}],
     "stateMutability": "nonpayable", "type": "function"},
    {"inputs": [{"name": "account", "type": "address"}],
     "name": "balanceOf", "outputs": [{"name": "", "type": "uint256"}],
     "stateMutability": "view", "type": "function"},
]

KEYSTORE_DIR = Path(os.environ.get("ZERO_ADS_KEYSTORE_DIR", Path.home() / ".0-ads" / "keys"))


def _derive_machine_password() -> str:
    """Derive a machine-specific password from hardware/user entropy. Not a substitute for a real password."""
    mac = str(uuid.getnode())
    user = getpass.getuser()
    raw = f"0-ads:{mac}:{user}:{KEYSTORE_DIR}".encode()
    return hashlib.sha256(raw).hexdigest()


def _get_persistent_wallet() -> tuple:
    """Load or create a persistent agent wallet stored in an encrypted keyfile."""
    KEYSTORE_DIR.mkdir(parents=True, exist_ok=True)
    keyfile = KEYSTORE_DIR / "agent_wallet.json"

    w3 = Web3()
    explicit_password = os.environ.get("ZERO_ADS_WALLET_PASSWORD")

    if explicit_password:
        password = explicit_password
    else:
        password = _derive_machine_password()
        logger.warning(
            "ZERO_ADS_WALLET_PASSWORD not set. Using a machine-derived password. "
            "Set ZERO_ADS_WALLET_PASSWORD for production security."
        )

    if keyfile.exists():
        with open(keyfile) as f:
            encrypted = json.load(f)
        private_key = w3.eth.account.decrypt(encrypted, password)
        account = w3.eth.account.from_key(private_key)
    else:
        private_key = "0x" + secrets.token_hex(32)
        account = w3.eth.account.from_key(private_key)
        encrypted = account.encrypt(password)
        with open(keyfile, "w") as f:
            json.dump(encrypted, f)
        os.chmod(keyfile, 0o600)
        logger.info(f"Created persistent agent wallet: {account.address}")

    return account, account.key


@mcp.tool()
def get_active_campaigns() -> str:
    """
    Fetch the list of active advertising campaigns from the 0-ads decentralized billboard.
    Agents can use this tool to discover what tasks are currently paying out USDC.
    """
    try:
        res = requests.get(BILLBOARD_URL, timeout=10)
        data = res.json()
        campaigns = data.get("campaigns", [])
        active = [c for c in campaigns if c.get("status") in ("active", "upcoming")]

        return json.dumps({
            "network": data.get("network"),
            "campaigns": active
        }, indent=2)
    except Exception as e:
        return f"Failed to fetch billboard: {str(e)}"


@mcp.tool()
def claim_gasless_bounty(campaign_id: str, agent_handle: str, target: str, payout_usdc: float, safe_address: str = "") -> str:
    """
    Claim a 0-ads programmatic bounty using the Gasless Relayer.
    Uses a persistent encrypted wallet. If safe_address is provided, earned USDC
    is automatically swept to that address after claiming.

    Args:
        campaign_id: The campaign hex ID (e.g. 0x0000...)
        agent_handle: Your username/ID on the platform (e.g. your GitHub ID, Twitter handle)
        target: The target entity of the campaign (repo name, tweet ID, etc)
        payout_usdc: Expected payout amount
        safe_address: (Optional) ERC-20 address to auto-sweep funds to after claim
    """
    try:
        agent_account, agent_key = _get_persistent_wallet()
        agent_address = agent_account.address

        bind_timestamp = int(time.time())
        msg = encode_defunct(text=f"0-ads-wallet-bind:{agent_handle}:{bind_timestamp}")
        wallet_sig = agent_account.sign_message(msg).signature.hex()

        payload = {
            "agent_github_id": agent_handle,
            "agent_id": agent_handle,
            "target": target,
            "target_repo": target,
            "chain_id": CHAIN_ID,
            "contract_addr": CONTRACT_ADDRESS,
            "campaign_id": campaign_id,
            "agent_eth_addr": agent_address,
            "payout": int(payout_usdc),
            "deadline": int(time.time()) + 3600,
            "wallet_sig": wallet_sig,
            "bind_timestamp": bind_timestamp
        }

        oracle_res = requests.post(ORACLE_URL, json=payload, timeout=10).json()
        if oracle_res.get("error"):
            return f"Oracle Verification Failed: {oracle_res['error']}"

        signature = oracle_res.get("signature", "")
        if not signature.startswith("0x"):
            signature = "0x" + signature

        relay_payload = {
            "campaign_id": campaign_id,
            "deadline": payload["deadline"],
            "oracle_signature": signature,
            "recipient": agent_address
        }

        relay_res = requests.post(RELAYER_URL, json=relay_payload, timeout=10).json()
        if relay_res.get("error"):
            return f"Relayer Execution Failed: {relay_res['error']}"

        tx_hash = relay_res.get("tx_hash", "0x...")

        sweep_msg = ""
        if safe_address:
            sweep_msg = _try_sweep_to_safe(agent_key, agent_address, safe_address)

        return (
            f"SUCCESS: Bounty claimed via Gasless Relayer.\n"
            f"Earned: {payout_usdc} USDC\n"
            f"Agent Wallet: {agent_address}\n"
            f"Transaction: https://sepolia.basescan.org/tx/{tx_hash}\n"
            f"{sweep_msg}"
        )
    except Exception as e:
        return f"Failed to claim bounty: {str(e)}"


def _try_sweep_to_safe(agent_key: bytes, agent_address: str, safe_address: str) -> str:
    """Attempt to sweep all USDC from the agent wallet to a user-provided safe address."""
    try:
        rpc_url = os.environ.get("RPC_URL", "https://sepolia.base.org")
        w3 = Web3(Web3.HTTPProvider(rpc_url))
        safe_addr = Web3.to_checksum_address(safe_address)

        token_addr = os.environ.get("USDC_TOKEN_ADDRESS")
        if not token_addr:
            return "Note: Set USDC_TOKEN_ADDRESS env var to enable auto-sweep."

        usdc = w3.eth.contract(address=Web3.to_checksum_address(token_addr), abi=USDC_ABI)
        balance = usdc.functions.balanceOf(Web3.to_checksum_address(agent_address)).call()

        if balance == 0:
            return "Sweep: No USDC balance to sweep (tx may still be pending)."

        tx = usdc.functions.transfer(safe_addr, balance).build_transaction({
            'from': Web3.to_checksum_address(agent_address),
            'nonce': w3.eth.get_transaction_count(Web3.to_checksum_address(agent_address)),
            'gas': 100_000,
            'gasPrice': w3.eth.gas_price,
            'chainId': int(os.environ.get("CHAIN_ID", 84532)),
        })
        signed = w3.eth.account.sign_transaction(tx, private_key=agent_key)
        sweep_hash = w3.eth.send_raw_transaction(signed.raw_transaction)
        return f"Sweep: {balance} USDC swept to {safe_address} (tx: {sweep_hash.hex()})"
    except Exception as e:
        return f"Sweep failed (funds remain in agent wallet): {e}"


if __name__ == "__main__":
    mcp.run(transport="stdio")
