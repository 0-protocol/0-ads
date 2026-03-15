import asyncio
import os
import sys
import json
import time
import requests
import secrets
from web3 import Web3
from eth_account.messages import encode_defunct

# Using FastMCP for simplicity
try:
    from mcp.server.fastmcp import FastMCP
except ImportError:
    # Fallback definition if library not installed, just for structural correctness
    print("Please pip install mcp", file=sys.stderr)
    sys.exit(1)

mcp = FastMCP("0-ads-bounty-hunter", description="Decentralized programmatic advertising network for Agent Economy")

ORACLE_URL = "https://ads.0-protocol.org/api/v1/oracle/verify"
RELAYER_URL = "https://ads.0-protocol.org/api/v1/relayer/execute"
BILLBOARD_URL = "https://raw.githubusercontent.com/0-protocol/0-ads/main/billboard.json"

CHAIN_ID = 84532
CONTRACT_ADDRESS = "0x8a2aD6bC4A240515c49035bE280BacB7CA94afC4"

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
def claim_gasless_bounty(campaign_id: str, agent_handle: str, target: str, payout_usdc: float) -> str:
    """
    Claim a 0-ads programmatic bounty using the Gasless Relayer.
    This tool auto-generates an ephemeral wallet, proves intent via the 0-ads Oracle, 
    and dispatches the transaction to the AdEscrow contract via the relayer network.
    
    Args:
        campaign_id: The campaign hex ID (e.g. 0x0000...)
        agent_handle: Your username/ID on the platform (e.g. your GitHub ID, Twitter handle)
        target: The target entity of the campaign (repo name, tweet ID, etc)
        payout_usdc: Expected payout amount
    """
    try:
        # Generate ephemeral wallet
        private_key = "0x" + secrets.token_hex(32)
        w3 = Web3()
        agent_account = w3.eth.account.from_key(private_key)
        agent_address = agent_account.address
        
        # Sign intent
        msg = encode_defunct(text=f"0-ads-wallet-bind:{agent_handle}")
        wallet_sig = agent_account.sign_message(msg).signature.hex()
        
        payload = {
            "agent_github_id": agent_handle, # Fallback naming convention
            "agent_id": agent_handle,
            "target": target,
            "target_repo": target,
            "chain_id": CHAIN_ID,
            "contract_addr": CONTRACT_ADDRESS,
            "campaign_id": campaign_id,
            "agent_eth_addr": agent_address,
            "payout": int(payout_usdc),
            "deadline": int(time.time()) + 3600,
            "wallet_sig": wallet_sig
        }
        
        # Call Oracle
        oracle_res = requests.post(ORACLE_URL, json=payload, timeout=10).json()
        if oracle_res.get("error"):
            return f"Oracle Verification Failed: {oracle_res['error']}"
            
        signature = oracle_res.get("signature", "")
        if not signature.startswith("0x"): signature = "0x" + signature
        
        # Call Relayer
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
        
        return (
            f"✅ SUCCESS! Bounty Claimed Successfully via Gasless Relayer.\n"
            f"💰 Earned: {payout_usdc} USDC\n"
            f"💼 Agent Ephemeral Wallet: {agent_address}\n"
            f"🔑 Private Key (Give to human to withdraw): {private_key}\n"
            f"🔗 Transaction Hash: https://sepolia.basescan.org/tx/{tx_hash}"
        )
    except Exception as e:
        return f"Failed to claim bounty: {str(e)}"

if __name__ == "__main__":
    mcp.run(transport="stdio")
