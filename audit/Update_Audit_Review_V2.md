# 0-ads Joint Security Audit Report (V2 Update)

**Auditors:** Joint Security Committee (OpenZeppelin, SlowMist, CertiK, Halborn, GoPlus)
**Audit Target:** `0-ads` Core Codebase (EVM/Solana Contracts, Rust Oracle Node)
**Audit Status:** Re-Audit on latest commits
**Audit Date:** March 14, 2026

---

## 1. Executive Summary

Following the latest code push by the development team, the Joint Security Committee conducted a second round of code auditing on `0-ads`. We are pleased to see the rapid response in fixing most of the high-risk vulnerabilities (e.g., EVM fee-on-transfer token lockup, Oracle update Rug Pull risk, Solana cross-deployment replay attacks, and Rust node global DoS).

However, **a critical cross-component cryptographic blocking vulnerability and a P2P network spam risk still remain in the system.** Below are detailed audit opinions from the perspectives of five top security firms.

---

## 2. Independent Auditor Perspectives

### 🛡️ CertiK (Focus on Cryptography & Cross-Chain Consistency)
**Status: [Critical Vulnerability Unfixed] Cross-Component Cryptographic Incompatibility**
- **Re-Audit Opinion:** Although the Solana smart contract added `program_id` to prevent cross-chain replays, **the Rust Oracle (`src/oracle.rs`) still hardcodes the `secp256k1` elliptic curve and the Ethereum message prefix (`\x19Ethereum Signed Message:\n32`) for signing.**
- **Impact:** Solana's `ed25519_program` can absolutely never verify a `secp256k1` signature. This means the current Rust backend and the Solana contract are **completely disconnected**. Agents can never claim their payouts on the Solana network.
- **Mandatory Fix:** The Oracle's `VerifyRequest` must differentiate by `chain_id`. When the target is Solana, it must use the `ed25519-dalek` library to generate native Ed25519 signatures, and the Payload must strictly match the Solana contract's expected `[program_id, campaign_id, agent, payout]`, removing any Ethereum prefixes.

### 🕵️‍♂️ SlowMist (Focus on P2P Networks & Off-Chain Attack/Defense)
**Status: [High Vulnerability Unfixed] P2P Fake Intent Spam (Griefing Attack)**
- **Re-Audit Opinion:** The development team successfully wired the Gossipsub `publish` broadcast mechanism via an `mpsc` channel in `main.rs`, which is an improvement. However, in the P2P message receiving logic, **on-chain state verification is still missing.**
- **Impact:** Malicious nodes can still broadcast massive amounts of forged, high-payout `AdIntent`s to the `0-ads-intents-v1` topic at zero cost. Upon receiving these, Agent nodes will consume local compute and GitHub API quotas, only to find the Campaign doesn't exist on-chain or has a 0 budget.
- **Mandatory Fix:** Before promoting `unverified_intents` to `active_intents`, an asynchronous RPC call (e.g., calling an Ethereum or Solana node) must be introduced to verify that the `campaign_id` genuinely exists and `remaining_budget >= payout`.

### 🏗️ OpenZeppelin (Focus on EVM Architecture & DeFi Composability)
**Status: [High Fixed / Medium Partially Remains] EVM Contract Review**
- **Re-Audit Opinion:**
  1. **Fee-on-Transfer Fix (Pass):** The introduction of the `actualBudget` logic perfectly resolves the ledger desynchronization issue caused by Fee-on-Transfer tokens.
  2. **Oracle Trust Crisis Fix (Pass):** The introduction of `previousOracle` and a 1-hour `ORACLE_GRACE_PERIOD` effectively prevents advertisers from Rug Pulling agent payouts by front-running oracle updates.
  3. **Campaign ID Front-running (Remains):** `createCampaign` still allows external inputs for custom `campaignId`s. While this can be mitigated off-chain via retries, there remains a Griefing risk where malicious users monitor the Mempool to hijack premium IDs. It is recommended to handle this at the business logic layer.

### 🔐 Halborn (Focus on Solana Ecosystem & Account Model)
**Status: [High Fixed] Solana Contract Review**
- **Re-Audit Opinion:**
  1. **Cross-Deployment Replay Fix (Pass):** The development team added `crate::id()` (Program ID) into the signature payload in `verify_ed25519_signature`. This is a textbook defense that completely blocks the path of replaying testnet signatures on the mainnet.
  2. **Oracle Pubkey Validation (Pass):** Added checks against `Pubkey::default()` and `system_program::id()`, preventing edge cases that could bypass signatures with empty public keys.
  3. **Architecture Note:** Currently, the PDA seed for `ClaimReceipt` is `[b"claimed", campaign_id, agent]`, meaning an Agent can only claim once per Campaign. If this matches the business design, it is secure; if multiple claims are needed in the future, a `nonce` or `epoch` seed must be introduced.

### 🛡️ GoPlus (Focus on API Security & Risk Control Strategies)
**Status: [Medium Fixed] Oracle API & Cache Risk Control**
- **Re-Audit Opinion:**
  1. **Global DoS Fix (Pass):** The introduction of `extract_rate_key` is very timely. Unauthenticated users are now rate-limited based on IP or Agent ID, resolving the global DoS vulnerability caused by the shared `"anonymous"` rate limit bucket.
  2. **Cache Poisoning Fix (Pass):** The `CacheKey` in `oracle.rs` has been completed with `payout` and `deadline`, and a `CACHE_TTL_SECS` (1 hour) expiration eviction mechanism was added, thoroughly resolving the risks of old signature reuse and memory leaks.

---

## 3. Final Conclusion

**The system is currently NOT ready for Mainnet.**

Although the security of the smart contract layer (EVM & Solana) has reached a high industrial standard, the **cryptographic implementation flaws in the Rust Oracle node** (raised by CertiK) cause a physical break in the cross-chain business logic. Furthermore, the **unverified broadcasting on the P2P network** (raised by SlowMist) will severely threaten the trust of early Agent participants.

**Mandatory Action Items Before Launch:**
1. Refactor `oracle.rs` in Rust to implement a dual-signature engine for `secp256k1` (EVM) and `ed25519` (Solana).
2. Add on-chain RPC verification logic to the Gossipsub message handling pipeline in `main.rs`.