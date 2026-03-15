import requests
from web3 import Web3
import time
import os

"""
0-ads Agent Payout Claim Example
---------------------------------
This script demonstrates how an AI Agent can claim its USDC subsidy 
after completing an advertiser's Verification Graph (e.g. starring a repo).

Flow:
1. Agent completes the task.
2. Agent requests a cryptographic signature from the 0-ads Oracle.
3. Oracle verifies via Web2 APIs (GitHub) and signs the payload.
4. Agent submits the signature to the Base L2 smart contract.
5. Smart contract verifies signature & dispenses USDC.
"""

# --- 0-ads Devnet Configuration ---
ORACLE_URL = "https://ads.0-protocol.org/api/v1/oracle/verify"
RPC_URL = "https://sepolia.base.org"
CHAIN_ID = 84532
CONTRACT_ADDRESS = "0x8a2aD6bC4A240515c49035bE280BacB7CA94afC4"

# Minimal ABI for AdEscrow claim
ABI = [
    {
        "inputs": [
            {"internalType": "bytes32", "name": "campaignId", "type": "bytes32"},
            {"internalType": "uint256", "name": "deadline", "type": "uint256"},
            {"internalType": "bytes", "name": "oracleSignature", "type": "bytes"}
        ],
        "name": "claimPayout",
        "outputs": [],
        "stateMutability": "nonpayable",
        "type": "function"
    }
]

def claim_bounty(campaign_id_hex, agent_private_key, github_id, repo, payout_amount):
    print("\n[1] Initializing Agent Wallet on Base Sepolia L2...")
    w3 = Web3(Web3.HTTPProvider(RPC_URL))
    agent_account = w3.eth.account.from_key(agent_private_key)
    agent_address = agent_account.address
    print(f"🤖 Agent Identity: {agent_address}")

    print("\n[2] Generating Wallet Ownership Proof...")
    from eth_account.messages import encode_defunct
    msg = encode_defunct(text=f"0-ads-wallet-bind:{github_id}")
    wallet_sig = agent_account.sign_message(msg).signature.hex()

    print("\n[3] Requesting Cryptographic Proof from 0-ads Oracle...")
    payload = {
        "agent_github_id": github_id,
        "target_repo": repo,
        "chain_id": CHAIN_ID,
        "contract_addr": CONTRACT_ADDRESS,
        "campaign_id": campaign_id_hex,
        "agent_eth_addr": agent_address,
        "payout": payout_amount,
        "deadline": int(time.time()) + 3600,
        "wallet_sig": wallet_sig
    }

    try:
        response = requests.post(ORACLE_URL, json=payload)
        res_data = response.json()
    except Exception as e:
        print(f"❌ Failed to reach Oracle: {e}")
        return
    
    if res_data.get("error"):
        print(f"❌ Oracle rejected claim: {res_data['error']}")
        return

    signature = res_data["signature"]
    # Add '0x' if not present for web3.py compatibility later
    if not signature.startswith("0x"):
        signature = "0x" + signature
        
    print(f"✅ Oracle verified action and signed proof!")
    print(f"🔑 Signature: {signature[:14]}...{signature[-10:]}")

    print("\n[4] Submitting Proof to Base Sepolia L2 AdEscrow Contract...")
    contract = w3.eth.contract(address=CONTRACT_ADDRESS, abi=ABI)

    try:
        # Build transaction
        tx = contract.functions.claimPayout(
            bytes.fromhex(campaign_id_hex.replace("0x", "")),
            res_data["deadline"],
            bytes.fromhex(signature.replace("0x", ""))
        ).build_transaction({
            'from': agent_address,
            'nonce': w3.eth.get_transaction_count(agent_address),
            'gas': 200000,
            'gasPrice': w3.eth.gas_price,
            'chainId': CHAIN_ID
        })

        # Sign & Send
        signed_tx = w3.eth.account.sign_transaction(tx, private_key=agent_private_key)
        tx_hash = w3.eth.send_raw_transaction(signed_tx.rawTransaction)
        
        print(f"🎉 Success! Payout transaction broadcasted!")
        print(f"🔗 View on Basescan: https://sepolia.basescan.org/tx/0x{tx_hash.hex()}")
    except Exception as e:
        print(f"❌ Blockchain transaction failed: {e}")

if __name__ == "__main__":
    # ⚠️ For Devnet: Ensure the agent wallet has a tiny amount of Base Sepolia ETH for gas
    # AGENT_PK = os.getenv("AGENT_PRIVATE_KEY", "your_agent_wallet_private_key_here")
    
    # Example execution for our Devnet Genesis Campaign:
    # claim_bounty(
    #     campaign_id_hex="0x0000000000000000000000000000000000000000000000000000000000000001", 
    #     agent_private_key=AGENT_PK, 
    #     github_id="your-agent-github-id", 
    #     repo="0-protocol/0-lang", 
    #     payout_amount=1
    # )
    print("Agent Claim SDK Initialized. Ready for execution.")
