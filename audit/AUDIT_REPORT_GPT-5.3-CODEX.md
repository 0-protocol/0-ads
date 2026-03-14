# 0-ads Security Audit Report (Second Re-Audit)

- Target repository: `0-ads`
- Initial audit date: 2026-03-14
- First re-audit date: 2026-03-14
- Second re-audit date: 2026-03-14
- Auditor: GPT-5.3-Codex

---

## 1. Scope and Context

This update re-audits the codebase after the latest remediation commit:

- `c87c15e` - `fix: resolve residual R-01 and R-02 for mainnet readiness`

Reviewed changes include:

- `src/main.rs`
- `contracts/evm/contracts/AdEscrow.sol`
- `contracts/evm/test/AdEscrow.test.js`
- `contracts/solana_anchor/src/lib.rs`

---

## 2. Updated Security Posture

- Previous rating (first re-audit): **Medium-Low**
- Current rating (this re-audit): **Low-Medium**

The two previously open residual items were addressed:

- R-01 (oracle rotation capability) -> implemented
- R-02 (fail-closed auth default) -> implemented

The project is materially improved and closer to mainnet readiness, but still has protocol-level residual risk around off-chain GitHub verification semantics.

---

## 3. Re-Audit Matrix

| ID | Prior Status | Current Status | Notes |
|---|---|---|---|
| C-01 | Resolved (with caveat) | **Resolved** | Fail-closed behavior now default in code path |
| C-02 | Resolved | **Resolved** | Wallet ownership binding remains in place |
| H-01 | Resolved | **Resolved** | Deadline bounded server-side |
| H-02 | Resolved | **Resolved** | Bounded intent/rate-limiter memory controls present |
| H-03 | Resolved | **Resolved** | Ed25519 parsing checks remain hardened |
| M-01 | Mitigated | **Mitigated** | `execute_graph` still feature-gated and disabled by default |
| M-02 | Resolved | **Resolved** | Client error sanitization retained |
| M-03 | Resolved | **Resolved** | Solana token-account constraints retained |
| L-01 | Resolved | **Resolved** | Immutable dependency pin + lockfile retained |
| L-02 | Resolved | **Resolved** | Lockfile baseline remains consistent |
| L-03 | Resolved | **Resolved** | Docker hardening retained |
| R-01 | Open | **Resolved** | Oracle rotation now supported on EVM and Solana paths |
| R-02 | Open | **Resolved** | `REQUIRE_AUTH` now defaults to true |

---

## 4. Verified New Fixes

### 4.1 R-01 Oracle key rotation support

**EVM (`AdEscrow.sol`)**

- Added `updateOracle(bytes32 campaignId, address newOracle)`
- Access control: advertiser-only (`c.advertiser == msg.sender`)
- Input validation: rejects zero-address oracle
- Emits `OracleUpdated(campaignId, oldOracle, newOracle)`

**Solana (`contracts/solana_anchor/src/lib.rs`)**

- Added `update_oracle(ctx, new_oracle_pubkey)`
- Account constraint includes `has_one = advertiser` on campaign account
- Advertiser signer required via `UpdateOracle` context

### 4.2 R-02 Fail-closed auth by default

**Rust node (`src/main.rs`)**

- `REQUIRE_AUTH` now defaults to enabled:
  - `.unwrap_or(true)`
- Explicit opt-out is required (`REQUIRE_AUTH=false` or `0`)
- Startup still refuses to run when auth required but `API_SECRET` missing

---

## 5. Test Evidence

- Re-ran `npm test` under `contracts/evm` after latest changes:
  - **20 passing**
  - Includes new `updateOracle` tests:
    - advertiser key rotation success
    - non-advertiser access rejection
    - zero address rejection
    - payout claim with rotated oracle

Note: Rust host compile stability can still vary by local toolchain/OS configuration; verify in CI and release image pipeline for production sign-off.

---

## 6. Remaining Risks

## Medium

### R-03 Off-chain GitHub verification remains TOCTOU-prone

- Location: `src/oracle.rs` (`verify_github_star`)
- Risk: a user can satisfy star state at verification time, obtain signature, then remove star later.
- This is a protocol/property limitation rather than a direct implementation flaw.
- Recommendation:
  - treat as accepted risk for current phase, and
  - evaluate stronger attestations (for example verifiable web proofs) for mainnet-grade assurance.

## Low

### R-04 Solana oracle rotation allows arbitrary pubkey assignment

- Location: `contracts/solana_anchor/src/lib.rs` (`update_oracle`)
- Observation: unlike EVM’s zero-address check, Solana path currently permits any `Pubkey` value.
- Impact: primarily operational (misconfiguration can break future claims until rotated again).
- Recommendation:
  - add explicit guardrails (for example deny known invalid sentinel values and add event logging for rotation metadata).

---

## 7. Final Recommendation

- **Testnet:** acceptable.
- **Mainnet:** near-ready from implementation perspective, but production launch should still include:
  - formal runbooks for oracle-key incident response,
  - CI-backed reproducible build checks,
  - explicit risk acceptance for R-03 or stronger proof mechanism rollout.

---

**Auditor Signature**  
GPT-5.3-Codex
