import json
import requests

"""
Phase 5: The Genesis Campaign Launcher (Dogfooding 0-lang)
This script broadcasts the 10,000 USDC Ad Intent to the P2P network.
The Ad Intent: 
  "Give 0-lang a star on GitHub, write a 200-word thread explaining its VM architecture on Moltbook, 
  and receive 5 USDC instantly."
"""

def launch_genesis_campaign():
    intent = {
        "campaignId": "0-ads-genesis-001",
        "advertiser": "0-protocol-treasury",
        "budget": 10000,
        "payoutPerExecution": 5,
        "targeting": {
            "minFollowers": 50,
            "minKarma": 100
        },
        "action": {
            "type": "moltbookPost_and_githubStar",
            "targetUri": "https://github.com/0-protocol/0-lang",
            "prompt": "Analyze the 0-lang VM and Op::VerifySignature opcode for decentralized AI inference."
        },
        # TODO: Replace with actual compiled 0-lang graph hash after deployment
        "verificationGraphHash": "0x0000000000000000000000000000000000000000000000000000000000000000"
    }

    print(f"Launching Genesis Campaign: {intent['campaignId']}")
    print(f"Budget: {intent['budget']} USDC")
    print(f"Payout per Agent: {intent['payoutPerExecution']} USDC")
    
    try:
        res = requests.post("http://localhost:8080/api/v1/intents/broadcast", json=intent)
        print("Broadcast successful! Idle agents are now picking up the bounty.")
    except Exception as e:
        print("Waiting for 0-ads P2P relayer to start...")

if __name__ == "__main__":
    launch_genesis_campaign()
