# Phase 3 Architecture: Future Features for 0-ads

This document proposes advanced features to make the 0-ads protocol production-grade and best-in-class for decentralized agent-native advertising.

---

## 1. Decentralized Oracle Network (DON)

### Problem
The entire protocol hinges on a single ECDSA key held by one Rust process. Oracle compromise = total fund loss (Playbook Delta in the audit).

### Proposed Architecture

```
Agent -> OracleNode_1 ---\
Agent -> OracleNode_2 ----+--> Threshold Signature (t-of-n) --> Single 65-byte sig
Agent -> OracleNode_3 ---/                                        |
                                                                  v
                                                          AdEscrow.sol verifies
                                                          as if single oracle
```

**Approach: Threshold ECDSA (t-of-n)**

- Deploy 5 oracle nodes with shares of a single secp256k1 key using a Threshold Signature Scheme (e.g., GG20, FROST for ECDSA).
- Any 3-of-5 nodes must independently verify the GitHub star (or other action) and contribute a signature share.
- The combined signature is indistinguishable from a regular ECDSA signature — zero changes to AdEscrow.sol.
- On-chain, the oracle address stays the same. Off-chain, compromise of 1-2 nodes is insufficient to sign.

**Key Design Decisions:**
- Use the FROST protocol (Flexible Round-Optimized Schnorr Threshold) adapted for ECDSA via k256.
- Nodes communicate via a private gossipsub topic with authenticated encryption.
- Distributed Key Generation (DKG) ceremony produces key shares — no single party ever sees the full key.
- Key rotation: run a proactive key resharing ceremony periodically (e.g., weekly).

**Migration Path:**
1. Deploy 5 oracle nodes with separate keys, each registered as `oracle` on a test campaign.
2. Run DKG to produce threshold shares.
3. Call `updateOracle` on all campaigns to point to the new threshold public key address.
4. Decommission the single-key oracle.

---

## 2. ZK-TLS for Trustless Verification

### Problem
The oracle makes GitHub API calls to verify agent actions. This is centralized trust — agents must trust the oracle to honestly report the API response. It also bottlenecks on GitHub's rate limit (BH-L04).

### Proposed Architecture

```
Agent's Browser/CLI
    |
    |-- 1. Star the repo on github.com (normal HTTPS)
    |-- 2. Fetch github.com/users/{id}/starred (TLS connection)
    |-- 3. Run TLS session through ZK-TLS prover (e.g., tlsnotary, zkTLS by Reclaim)
    |
    v
ZK Proof: "This TLS session with github.com (verified by its certificate chain)
           returned a response body containing exactly full_name == 0-protocol/0-lang
           in the starred repos array"
    |
    v
Oracle verifies the ZK proof (no GitHub API call needed)
    |
    v
Signs payout
```

**Benefits:**
- Oracle never touches GitHub API — no rate limits.
- Agent privacy: the oracle only sees the proof, not the full API response.
- Trustless: the proof is cryptographically bound to GitHub's TLS certificate.
- Scalable: verification is local compute, not network-bound.

**Implementation Notes:**
- Use TLSNotary (Rust-based) or Reclaim Protocol for proof generation.
- The ZK proof verifier runs inside the oracle as a Rust library.
- Proof size is ~1-5 KB; verification takes ~50ms — well within the oracle's latency budget.
- The `VerifyRequest` payload gains a new `zk_proof` field alongside (or replacing) `target_repo`.

---

## 3. UUPS Upgradeable Contract

### Problem
AdEscrow.sol is non-upgradeable. A post-mainnet vulnerability requires pausing, deploying a new contract, and migrating all campaigns (BH-I02).

### Proposed Architecture

```
Proxy (ERC1967)           Implementation V1           Implementation V2
+-----------------+       +-----------------+         +-----------------+
| delegatecall  --+------>| createCampaign  |         | createCampaign  |
| storage slots   |       | claimPayout     |   =>    | claimPayout     |
| admin: multisig |       | cancelCampaign  |         | cancelCampaign  |
+-----------------+       +-----------------+         | sweepDust (new) |
                                                      | disputeClaim    |
                                                      +-----------------+
```

**Approach: UUPS (ERC1967 + ERC1822)**

- Deploy a UUPS proxy with the current AdEscrow logic as Implementation V1.
- The `_authorizeUpgrade` function is guarded by a multisig (Gnosis Safe).
- All existing campaigns, mappings, and state live in the proxy's storage slots.
- Upgrades deploy a new implementation and call `upgradeTo(newImpl)`.
- Storage layout must be append-only — new fields go at the end.

**Safety Measures:**
- Timelock (48h) on upgrades so the community can review.
- Storage gap of 50 slots reserved in V1 for future fields.
- Automated storage layout checker in CI.
- Emergency `upgradeToAndCall` for critical patches (bypasses timelock via 4-of-7 multisig).

---

## 4. Dispute / Fraud Proof Mechanism

### Problem
Once the oracle signs and the agent claims, funds are irreversibly gone. A single oracle bug causes unrecoverable fund loss (BH-I01).

### Proposed Architecture

```
Agent claims -> Funds enter 24h escrow hold -> If no dispute -> Auto-release
                                             -> If disputed -> Arbitration
```

**Two-Phase Claim:**

1. **Claim Phase**: Agent calls `claimPayout`. Instead of immediate transfer, funds move to a `PendingClaim` struct with a 24-hour release timestamp.
2. **Challenge Phase**: During 24 hours, the advertiser (or a designated watcher) can call `disputeClaim(campaignId, agent, evidence)`. This freezes the claim and emits a `ClaimDisputed` event.
3. **Resolution Phase**: An arbitrator (initially the protocol multisig, later a decentralized court like Kleros) reviews and calls `resolveClaim(campaignId, agent, approved)`.
4. **Release Phase**: After 24 hours with no dispute, anyone can call `releaseClaim(campaignId, agent)` to finalize the transfer.

**Tradeoffs:**
- Agents experience a 24-hour delay instead of instant settlement.
- Gas cost increases (two transactions instead of one).
- Could offer a "fast lane" for campaigns that opt out of dispute protection (advertiser accepts the risk).

---

## 5. Real-time Indexing & Monitoring (Subgraph)

### Problem
No real-time alerting, no dashboards, no anomaly detection. Forensics requires manual log parsing (BH-I04).

### Proposed Architecture

**Subgraph (The Graph) for event indexing:**

Entities:
- `Campaign`: id, advertiser, budget, payout, oracle, status, createdAt
- `Claim`: id, campaign, agent, amount, timestamp, txHash
- `OracleRotation`: campaign, oldOracle, newOracle, timestamp

Queries:
- Top campaigns by remaining budget
- Claims per hour (anomaly detection)
- Agent claim history
- Oracle rotation timeline

**Monitoring Service:**

A lightweight Python/Rust service that subscribes to Subgraph webhooks and triggers alerts:

- **Anomaly: Burst claims** — If a campaign receives >10 claims in 5 minutes, alert.
- **Anomaly: Single agent multi-campaign** — If one agent claims from >5 campaigns in 1 hour, flag for Sybil.
- **Health: Relayer balance** — Alert when relayer ETH drops below threshold.
- **Health: Oracle availability** — Ping oracle every 30s; alert on 3 consecutive failures.

Alerts route to: Telegram bot, Discord webhook, PagerDuty.

---

## Implementation Priority

| Feature | Effort | Impact | Priority |
|---------|--------|--------|----------|
| Subgraph + Monitoring | 1-2 weeks | Operational visibility | P0 (launch day) |
| UUPS Proxy | 1 week | Safety net for patches | P0 (launch day) |
| Dispute Mechanism | 2-3 weeks | Fund recovery capability | P1 (month 1) |
| Decentralized Oracle | 4-6 weeks | Eliminate SPOF | P1 (month 2) |
| ZK-TLS Verification | 6-8 weeks | Trustless + scalable | P2 (month 3+) |
