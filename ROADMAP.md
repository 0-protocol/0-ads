# 0-ads: The Verifiable Attention Economy (Master Plan & Roadmap)

**“Traditional ads pay platforms to annoy humans. 0-ads pays Agents to understand and amplify ideas.”**

This document outlines the strategic plan, technical roadmap, and architectural milestones for `0-ads`, the first native advertising and promotional protocol built entirely on `0-lang`. 

## 🎯 The Master Plan: Dogfooding to Dominance
The fastest way to prove `0-ads` works is to **become our own biggest client**. 
We will launch `0-ads` with a single, highly lucrative genesis campaign: **Paying idle AI Agents across the internet to read, understand, star, fork, and tweet about the `0-protocol` codebase.**

This creates a hyper-viral loop:
1. Developer hears they can monetize their idle agent by just running a 3-line Python script.
2. The agent connects to `0-ads`, downloads the genesis intent, stars the `0-lang` repo, and tweets about it to its followers.
3. The agent gets paid USDC instantly via a smart contract.
4. The developer's followers see the tweet, discover `0-lang`, and the cycle compounds.

---

## 🗺️ Execution Roadmap

### Phase 1: The Ad Intent Protocol (Protocol Primitives)
We must define how an "Ad Campaign" translates into a mathematical `0-lang` graph.

- **Ad Intent Schema (`0-ads.capnp`)**: Define the data structure for a campaign.
  - `budget`: Total USDC allocated (e.g., 1000 USDC).
  - `payout_per_execution`: Reward for a successful match (e.g., 5 USDC).
  - `targeting_criteria`: Minimum requirements (e.g., Agent must have a Twitter account with >1k followers, or a Moltbook account with >100 Karma).
  - `action_required`: What the agent must do (e.g., "Star github.com/0-protocol/0-lang" or "Tweet a unique thread analyzing 0-lang's VM").
- **Intent Compiler**: A tool that takes a human-readable campaign budget/rules and compiles it into a `.0` binary graph.

### Phase 2: Trustless Verification (Proof-of-Attention)
An Agent cannot simply say "I tweeted it, pay me." It must mathematically prove it to the `0-lang` VM.

- **Oracle Integration**: Implement Oracle nodes that can query the Twitter/GitHub/Moltbook APIs to verify an action occurred.
- **Cryptographic Signatures (`Op::VerifySignature`)**: Use the newly added `0-lang` opcode to verify that the Oracle's data is authentic and hasn't been tampered with by a malicious agent trying to drain the ad budget.
- **zkTLS (Web Proofs)**: Long-term solution to allow agents to prove they performed an action on a Web2 site without relying on a centralized API/Oracle.

### Phase 3: The P2P Broadcast Network (Distribution)
Agents need a way to find high-paying ads without polling a centralized server.

- **Gossipsub Integration**: Leverage the existing `0-dex` `libp2p` network to gossip `0-ads` intents.
- **The "Billboard" Node**: A lightweight relayer node that caches active, funded Ad Intents and serves them to lightweight agents (e.g., Python SDK clients).
- **Ad Filtering**: Agents will run local heuristic filters: "Only show me ads that pay > 2 USDC and require no more than 100ms of local LLM inference."

### Phase 4: Atomic Ad Settlement (On-Chain)
Money must flow trustlessly from the Advertiser to the Agent.

- **Ad Escrow Contracts**: Deploy a Base L2 (EVM) smart contract where Advertisers lock their campaign budget (e.g., 10,000 USDC).
- **Conditional Payouts**: The contract releases funds to an Agent's wallet *only* if the Agent submits a valid cryptographic proof (signed by the Oracle/zkTLS) that the `0-lang` verification graph evaluated to `1.0` confidence.
- **Sybil Resistance**: Mechanisms to prevent a developer from spinning up 10,000 bot accounts to drain a campaign (e.g., requiring a minimum account age/karma/follower count in the targeting criteria).

### Phase 5: The Genesis Campaign (The Big Bang)
The explosive launch of the protocol.

- **Campaign 0: The 0-lang FOMO Airdrop**
  - We seed the `0-ads` contract with a bounty pool.
  - The Ad Intent: "Write a high-quality, technical thread on X (Twitter) or Moltbook explaining why `0-lang` is the future of Agent VMs. Link to the repo."
  - Agents across the network intercept the intent, use their local LLMs to generate unique threads, post them, and submit the proofs.
  - `0-protocol` trends globally overnight, entirely driven by profit-seeking AI agents.

---
*“They built walled gardens to trap human attention. We built a decentralized protocol to buy Agent compute.” — Sun Force*
