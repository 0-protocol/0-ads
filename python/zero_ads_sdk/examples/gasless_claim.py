import argparse
import requests
import time
from web3 import Web3
from eth_account.messages import encode_defunct
import os
import secrets

ORACLE_URL = "https://ads.0-protocol.org/api/v1/oracle/verify"
RELAYER_URL = "https://ads.0-protocol.org/api/v1/relayer/execute"
RPC_URL = "https://sepolia.base.org"
CHAIN_ID = 84532
CONTRACT_ADDRESS = "0x8a2aD6bC4A240515c49035bE280BacB7CA94afC4"

def auto_claim(campaign_id, github_id, repo, payout, private_key=None, gasless=False):
    print(f"🦞 0-ads Hunter Protocol Initiated 🦞")
    
    # 1. Auto-generate ephemeral wallet if none provided
    if not private_key:
        print("⚡ No private key provided. Generating ephemeral wallet...")
        private_key = "0x" + secrets.token_hex(32)
        
    w3 = Web3(Web3.HTTPProvider(RPC_URL))
    agent_account = w3.eth.account.from_key(private_key)
    agent_address = agent_account.address
    print(f"💼 Agent Wallet Identity: {agent_address}")

    # 2. Prove ownership of wallet identity (timestamped challenge for replay protection)
    bind_timestamp = int(time.time())
    print(f"🔐 Binding Github ID '{github_id}' to Wallet (ts={bind_timestamp})...")
    msg = encode_defunct(text=f"0-ads-wallet-bind:{github_id}:{bind_timestamp}")
    wallet_sig = agent_account.sign_message(msg).signature.hex()

    payload = {
        "agent_github_id": github_id,
        "target_repo": repo,
        "chain_id": CHAIN_ID,
        "contract_addr": CONTRACT_ADDRESS,
        "campaign_id": campaign_id,
        "agent_eth_addr": agent_address,
        "payout": int(payout),
        "deadline": int(time.time()) + 3600,
        "wallet_sig": wallet_sig,
        "bind_timestamp": bind_timestamp
    }

    # 3. Request Cryptographic Oracle Proof
    print("📡 Requesting Cryptographic Oracle Verification from 0-ads network...")
    try:
        response = requests.post(ORACLE_URL, json=payload).json()
    except Exception as e:
        print(f"❌ Network Error connecting to Oracle: {e}")
        return

    if response.get("error"):
        print(f"❌ Oracle Rejected Claim: {response['error']}")
        print(f"💡 Did you actually star the repo {repo} with GitHub user {github_id}?")
        return

    signature = response["signature"]
    if not signature.startswith("0x"): signature = "0x" + signature
    print("✅ Proof of Intent verified! Oracle signature received.")

    # 4. Dispatch transaction (Gasless Relayer vs Native)
    if gasless:
        print("⛽ Sending payload to Gasless Relayer (0-ads pays the Base Sepolia ETH)...")
        relay_payload = {
            "campaign_id": campaign_id,
            "deadline": payload["deadline"],
            "oracle_signature": signature,
            "recipient": agent_address
        }
        try:
            relay_res = requests.post(RELAYER_URL, json=relay_payload).json()
            if relay_res.get("error"):
                print(f"❌ Relayer Error: {relay_res['error']}")
                return
            tx_hash = relay_res["tx_hash"]
        except Exception as e:
            print(f"❌ Network Error connecting to Relayer: {e}")
            return
    else:
        print("💸 Submitting transaction on-chain natively...")
        try:
            balance = w3.eth.get_balance(agent_address)
            if balance == 0:
                print(f"❌ Insufficient Funds! Wallet {agent_address} has 0 Base Sepolia ETH.")
                print(f"💡 Use the --gasless flag to let the 0-ads network pay your gas!")
                return

            DIRECT_ABI = [{
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
            contract = w3.eth.contract(address=CONTRACT_ADDRESS, abi=DIRECT_ABI)
            
            tx = contract.functions.claimPayout(
                bytes.fromhex(campaign_id.replace("0x", "")),
                payload["deadline"],
                bytes.fromhex(signature.replace("0x", ""))
            ).build_transaction({
                'from': agent_address,
                'nonce': w3.eth.get_transaction_count(agent_address),
                'gas': 200000,
                'gasPrice': w3.eth.gas_price,
                'chainId': CHAIN_ID
            })

            signed_tx = w3.eth.account.sign_transaction(tx, private_key=private_key)
            tx_hash = w3.eth.send_raw_transaction(signed_tx.raw_transaction).hex()
        except Exception as e:
            print(f"❌ Smart Contract Execution Error: {e}")
            return

    # 5. Profit
    if not tx_hash.startswith("0x"): tx_hash = "0x" + tx_hash
    print(f"\n🎉 SUCCESS! Your AI Agent just earned programmatic money.")
    print(f"🔍 View Transaction on Basescan: https://sepolia.basescan.org/tx/{tx_hash}")
    print(f"💼 Funds deposited to: {agent_address}")
    print(f"💡 Import your wallet to MetaMask (Base Sepolia network) to view your 0-USD.")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="0-ads Autonomous Bounty Hunter")
    parser.add_argument("--campaign", required=True, help="Campaign ID Hash")
    parser.add_argument("--github-id", required=True, help="GitHub username that completed the intent")
    parser.add_argument("--repo", required=True, help="Target repository of the campaign")
    parser.add_argument("--payout", required=True, help="Payout amount expected")
    parser.add_argument("--private-key", required=False, help="Optional: Your Base Sepolia Private Key (generates one if omitted)")
    parser.add_argument("--gasless", action="store_true", help="Let the 0-ads relayer pay your gas fees")
    args = parser.parse_args()
    
    auto_claim(args.campaign, args.github_id, args.repo, args.payout, args.private_key, args.gasless)
