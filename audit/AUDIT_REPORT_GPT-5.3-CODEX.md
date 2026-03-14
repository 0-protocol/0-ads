# 0-ads Security Audit Report (Post-Remediation Re-Audit)

- Target repository: `0-ads`
- Initial audit date: 2026-03-14
- Re-audit date: 2026-03-14
- Method: static code review + targeted verification checks
- Auditor: GPT-5.3-Codex

---

## 1. Executive Summary

This is a re-audit after your remediation commit `22a1291` (`fix: remediate GPT-5.3-Codex independent audit (2C/3H/3M/3L)`).

I re-verified the previously reported issues in:

- `src/main.rs`
- `src/oracle.rs`
- `contracts/solana_anchor/src/lib.rs`
- `Cargo.toml` and `Cargo.lock`
- `Dockerfile`
- `contracts/evm/package-lock.json`

### Updated overall risk

- Previous rating: **High**
- Current rating: **Medium-Low** (testnet posture), still **not mainnet-ready**

Most of the high-impact findings from the prior report are fixed. The key remaining risk is centralized trust in a single oracle signing key plus operational misconfiguration risk if auth is not strictly enforced outside hardened container defaults.

---

## 2. Re-Audit Result Matrix

| ID | Previous Severity | Finding | Status |
|---|---|---|---|
| C-01 | Critical | API auth fail-open when `API_SECRET` missing | **Resolved (with config caveat)** |
| C-02 | Critical | No binding between GitHub identity and wallet | **Resolved** |
| H-01 | High | Unbounded caller-controlled `deadline` | **Resolved** |
| H-02 | High | Unbounded in-memory growth / DoS | **Resolved (partially optimized, acceptable)** |
| H-03 | High | Weak Ed25519 instruction parsing checks | **Resolved** |
| M-01 | Medium | `execute_graph` returns mocked success | **Mitigated (disabled by default)** |
| M-02 | Medium | Internal error details leaked to clients | **Resolved** |
| M-03 | Medium | Loose Solana token account constraints | **Resolved** |
| L-01 | Low | Mutable Rust git dependency + missing lockfile | **Resolved** |
| L-02 | Low | NPM lock baseline drift | **Resolved** |
| L-03 | Low | Weak Docker hardening baseline | **Resolved** |

---

## 3. Verified Fixes

### 3.1 Authentication hardening (C-01)

- `REQUIRE_AUTH` is now supported and startup fails if auth is required but `API_SECRET` is absent.
- `Dockerfile` sets `REQUIRE_AUTH=true` by default, making containerized deployments fail-closed.

### 3.2 Identity-to-wallet binding (C-02)

- `verify_proof` now requires `wallet_sig`.
- `src/oracle.rs` adds signature recovery (`verify_wallet_ownership`) over challenge:
  - `0-ads-wallet-bind:{github_id}`
- Recovered signer is matched to `agent_eth_addr` before oracle signing.

### 3.3 Deadline policy controls (H-01)

- `verify_proof` now rejects:
  - past deadlines
  - deadlines beyond `now + MAX_SIGNATURE_DEADLINE_SECS` (1 hour cap)

### 3.4 DoS/memory controls (H-02)

- Added hard caps:
  - `MAX_ACTIVE_INTENTS = 10_000`
  - `MAX_UNVERIFIED_INTENTS = 5_000`
- Added stale key eviction for rate limiter windows (`evict_stale`).

### 3.5 Solana Ed25519 validation hardening (H-03)

- `verify_ed25519_signature` now validates:
  - padding byte
  - instruction index semantics (`u16::MAX` checks)
  - bounds via `saturating_add`
  - exact message length match against expected payload

### 3.6 Medium/Low fixes

- `execute_graph` endpoint is feature-gated (`ENABLE_GRAPH_EXECUTION`) and disabled by default.
- API responses now return sanitized client errors; internals logged server-side.
- Solana token accounts now include owner/mint constraints in campaign/claim/cancel flows.
- `zerolang` dependency is pinned to immutable git `rev` and `Cargo.lock` exists.
- `Dockerfile` now pins Rust toolchain image, uses non-root runtime user, and builds with `--locked`.

---

## 4. Residual Risks (Still Open)

## High

### R-01 Single oracle key trust concentration

- Location: `src/oracle.rs`
- Risk: one key compromise can authorize fraudulent signatures protocol-wide for campaigns trusting that oracle.
- Recommendation:
  - move to multi-oracle threshold signatures, or
  - introduce on-chain key rotation with emergency revocation procedures.

## Medium

### R-02 Auth policy can still be weakened by non-container deployment choices

- Location: `src/main.rs` (`REQUIRE_AUTH` default false)
- Risk: non-container operators can still run unauthenticated mode unless deployment policy enforces `REQUIRE_AUTH=true`.
- Recommendation:
  - consider fail-closed by default in code, or
  - enforce strict deployment policy + startup guards in all environments.

### R-03 Off-chain GitHub verification remains TOCTOU-prone

- Location: `src/oracle.rs` (`verify_github_star`)
- Risk: user can satisfy star check transiently, obtain signature, then unstar.
- Recommendation:
  - document this as accepted risk for testnet,
  - evaluate stronger attestation proofs (for example verifiable web proofs) for production.

---

## 5. Verification Notes

I ran targeted checks during this re-audit:

- `npm test` in `contracts/evm` -> **16 passing**
- `cargo check --locked` -> **failed due to local/toolchain environment issue while compiling `ring` C code on host**, not a direct logic regression in your remediation diff

Given the Rust build environment failure, I recommend validating in CI or a known-good toolchain image before release tagging.

---

## 6. Updated Recommendation

- **Testnet:** acceptable with monitoring and strict deployment configuration.
- **Mainnet:** do not proceed until residual high/medium risks are addressed, especially oracle key centralization and hard fail-closed auth policy across all deployment modes.

---

**Auditor Signature**  
GPT-5.3-Codex
