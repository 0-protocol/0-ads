# 0-ads V4 Security Remediation Report

**Remediation Engineer:** 0-ads Engineering  
**Date:** 2026-03-14  
**Base Commit:** `a504942` (post-Solana sunset)  
**Scope:** Cross-audit remediation covering all 4 prior audit reports  

---

## Audit Sources Reviewed

| Report | Auditor | Key Focus |
|--------|---------|-----------|
| `Full_System_Audit_Report.md` | OpenZeppelin + SlowMist | Full-stack architecture, P2P, oracle |
| `AdEscrow_Audit_Report.md` | OpenZeppelin + SlowMist | EVM contract deep-dive |
| `AUDIT_REPORT.md` (V3) | Claude Opus 4.6 | 3-round iterative audit |
| `AUDIT_REPORT_GPT-5.3-CODEX.md` | GPT-5.3-Codex | Independent second opinion |

---

## Remediation Summary

| ID | Severity | Finding | Source | Status | Fix |
|----|----------|---------|--------|--------|-----|
| FOT-01 | High | Fee-on-transfer token compatibility | AdEscrow 3.1, Full_System 4.2 | **RESOLVED** | Balance-diff accounting in `createCampaign` |
| ORC-01 | Medium | Oracle update race condition / rug pull | AdEscrow 3.3, Full_System 4.2 | **RESOLVED** | Grace period with `previousOracle` + `oracleUpdatedAt` |
| EXS-01 | Low | Missing explicit campaign existence check | AdEscrow 2.2 | **RESOLVED** | `require(c.advertiser != address(0))` in `claimPayout` |
| ORC-02 | Medium | Oracle zero-address in `createCampaign` | New finding | **RESOLVED** | `require(oracle != address(0))` |
| ORC-03 | Medium | No-op oracle update allowed | New finding | **RESOLVED** | `require(newOracle != c.oracle)` |
| GOS-01 | High | broadcast_intent not publishing to Gossipsub | Full_System 3.2 | **RESOLVED** | `mpsc::channel` bridge to swarm event loop |
| RAT-01 | Medium | Global "anonymous" rate limit DoS | Full_System 3.3 | **RESOLVED** | Per-agent/per-IP rate key extraction |
| CAC-01 | Medium | Signature cache key excludes deadline/payout | Full_System 3.4 | **RESOLVED** | `CacheKey` includes all signing parameters |
| CAC-02 | Low | Signature cache has no TTL | Full_System 3.4 | **RESOLVED** | TTL-based expiry + periodic eviction |
| SOL-* | — | All Solana findings | Multiple | **N/A** | Solana integration sunset in `a504942` |

---

## Detailed Changes

### 1. EVM Contract (`AdEscrow.sol`)

**Fee-on-Transfer Handling (FOT-01)**

Records `balanceBefore`/`balanceAfter` in `createCampaign` and uses the actual received delta as `budget`. This prevents accounting drift when deflationary tokens are used. A `MockFeeToken` (1% fee) was added for test coverage.

**Oracle Update Grace Period (ORC-01)**

Added `previousOracle` and `oracleUpdatedAt` fields to `Campaign` struct, plus a 1-hour `ORACLE_GRACE_PERIOD` constant. During `claimPayout`, if the current oracle check fails, the contract checks whether the signer matches `previousOracle` within the grace window. This prevents advertiser rug pulls where the oracle is changed to invalidate an agent's in-flight signature.

**Explicit Existence Check (EXS-01)**

`claimPayout` now starts with `require(c.advertiser != address(0), "Campaign does not exist")`, following the "fail early, fail loud" principle.

**Input Validation (ORC-02, ORC-03)**

- `createCampaign`: rejects `oracle == address(0)`
- `updateOracle`: rejects `newOracle == c.oracle` (no-op) and `address(0)`

**Test Coverage:** 26 passing tests (6 new: fee-on-transfer, existence check, same-oracle rejection, grace period acceptance, grace period expiry, zero-address oracle).

### 2. Rust Billboard Node (`src/main.rs`)

**Gossipsub Publish (GOS-01)**

Added `mpsc::unbounded_channel` between the HTTP handler and the swarm event loop. When `broadcast_intent` is called, it serializes the intent and sends it through the channel. The main event loop receives from this channel and calls `swarm.behaviour_mut().publish()` to the `0-ads-intents-v1` topic.

**Per-Identity Rate Limiting (RAT-01)**

Replaced the hardcoded `"anonymous"` fallback with `extract_rate_key()` which uses a priority-based key:
1. `x-api-key` header → `apikey:{key}`
2. Request-specific identifier (e.g. `agent_github_id`) → `agent:{id}`
3. `x-forwarded-for` / `x-real-ip` headers → `ip:{addr}`
4. Fallback → `anon:unknown`

This prevents a single attacker from exhausting the rate bucket for all unauthenticated users.

### 3. Oracle (`src/oracle.rs`)

**Cache Key Redesign (CAC-01)**

Replaced `(String, String)` cache key with a proper `CacheKey` struct containing `campaign_id`, `agent_eth_addr`, `payout`, and `deadline`. A request with a different deadline or payout now correctly generates a fresh signature instead of returning a stale cached one.

**Cache TTL (CAC-02)**

Added `CacheEntry` with `created_at: Instant` timestamp and a 1-hour TTL constant. Cache lookups check staleness before returning. A new `evict_expired_cache()` method is called every ~5 minutes from the background task loop.

---

## Remaining Acknowledged Risks

| ID | Severity | Description | Mitigation |
|----|----------|-------------|------------|
| H-06 | High | Single oracle ECDSA key | Deferred to multi-oracle milestone; key zeroized on drop |
| M-03 | Medium | GitHub star TOCTOU | Protocol limitation; zkTLS on roadmap |
| N-03 | Info | No on-chain campaign verification for gossipsub intents | Acceptable for testnet; claim fails at contract level |

---

## Test Evidence

```
AdEscrow: 26 passing (864ms)
  - 7 createCampaign tests (incl. fee-on-transfer, zero-oracle)
  - 7 claimPayout tests (incl. existence check, exhaustion)
  - 5 cancelCampaign tests
  - 7 updateOracle tests (incl. grace period acceptance/rejection)
```
