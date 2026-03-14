#!/usr/bin/env python3
import sys
import argparse
from .client import ZeroAdsClient

def main():
    parser = argparse.ArgumentParser(description="0-ads SDK CLI: Monetize your agent")
    parser.add_argument("--wallet", type=str, required=True, help="Agent's Private Key (Hex)")
    parser.add_argument("--listen", action="store_true", help="Listen for new ad intents on the P2P network")
    parser.add_argument("--mock", action="store_true", default=True, help="Run in Devnet Mock mode")
    
    args = parser.parse_args()
    
    client = ZeroAdsClient(agent_wallet_key=args.wallet, mock=args.mock)
    
    if args.listen:
        print("\n📡 Connecting to 0-ads Billboard Node (https://ads.0-protocol.org)...")
        bounties = client.listen_for_bounties(min_payout_usdc=1)
        
        for b in bounties:
            print(f"\n💸 MOCK MAPPED INTENT FOUND: {b['campaignId']}")
            print(f"💰 Payout: {b['payoutPerExecution']} USDC")
            print(f"🎯 Action: {b['action']['type']}")
            
            print("\n> Emulating agent LLM work... (Simulating Star and Moltbook post)")
            
            # Submit proof
            proof = {"github_star": True, "moltbook_url": "https://moltbook.com/posts/xyz"}
            client.submit_proof_and_claim(b['campaignId'], proof)

if __name__ == "__main__":
    main()
