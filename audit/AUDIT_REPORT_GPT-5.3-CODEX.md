# 0-ads Security Audit Report (Independent Review)

- Target repository: `/Users/JiahaoRBC/Git/0-ads`
- Audit date: 2026-03-14
- Method: Static security review (Rust/EVM/Solana/dependencies/deployment)
- Conclusion: Current version should not be used on production mainnet without remediation
- Auditor: GPT-5.3-Codex

---

## 1. Executive Summary

This review covers the core runtime (`src/main.rs`, `src/oracle.rs`, `src/network.rs`), EVM contract (`contracts/evm/contracts/AdEscrow.sol`), Solana Anchor program (`contracts/solana_anchor/src/lib.rs`), and supply-chain/deployment posture (`Cargo.toml`, `package*.json`, `Dockerfile`).

The dominant risks are:

1. Weak authentication/authorization boundaries (`API_SECRET` fail-open, no identity-to-wallet binding).
2. Signature lifetime and replay risk (`deadline` fully caller-controlled, reusable signature patterns).
3. Availability and supply-chain risk (unbounded in-memory structures, lockfile drift, weak container hardening).

Overall risk rating: **High**.

---

## 2. Threat Model and Attack Surface

### 2.1 Adversary Model

- Unauthenticated external callers hitting HTTP APIs directly.
- Malicious agents attempting forged or replayed payout claims.
- Supply-chain attackers exploiting mutable dependencies and lock drift.
- Infrastructure attackers leveraging deployment misconfiguration and container privilege defaults.

### 2.2 Attack Surface

- Oracle APIs: `/api/v1/oracle/verify`, `/api/v1/oracle/execute_graph`
- Intent APIs: `/api/v1/intents`, `/api/v1/intents/broadcast`
- EVM settlement flow: `createCampaign`, `claimPayout`, `cancelCampaign`
- Solana settlement flow: `create_campaign`, `claim_payout`, `cancel_campaign`, Ed25519 instruction verification
- Supply-chain/deployment: Cargo/NPM dependency controls, Docker runtime baseline, key handling

---

## 3. Findings (Sorted by Severity)

## Critical

### C-01 Authentication fails open when `API_SECRET` is missing

- Location: `src/main.rs` (`check_api_key`, `main`, `verify_proof`, `verify_graph_execution`)
- Issue: `check_api_key` returns success when `API_SECRET` is not configured.
- Impact: Oracle endpoints can be called anonymously in misconfigured environments.
- Remediation:
  - Enforce fail-closed startup for production when auth secret is absent.
  - Add explicit required auth mode (for example `REQUIRE_AUTH=true`) and deployment preflight checks.

### C-02 No binding between GitHub identity and payout wallet

- Location: `src/main.rs` (`verify_proof`), `src/oracle.rs` (`verify_github_star`)
- Issue: Verification checks GitHub star state, but signs caller-provided `agent_eth_addr` without ownership linkage.
- Impact: Attackers can satisfy GitHub condition with one identity and receive payout signatures for another wallet.
- Remediation:
  - Introduce verifiable account linkage (OAuth identity + wallet challenge signature).
  - Enforce a persisted `(github_id -> wallet)` mapping before signing.

## High

### H-01 Caller-controlled `deadline` without policy bounds

- Location: `src/main.rs` (`verify_proof`), `src/oracle.rs` (`sign_payout`)
- Issue: No server-side max/min validity window for `deadline`.
- Impact: Signatures can be issued with overly long validity windows, increasing replay and key-exposure blast radius.
- Remediation:
  - Enforce `now <= deadline <= now + MAX_TTL`.
  - Add nonce/claim-id semantics and one-time claim enforcement.

### H-02 Multiple unbounded memory structures enable DoS

- Location: `src/main.rs` (`active_intents`, `unverified_intents`, `SlidingWindowRateLimiter.windows`)
- Issue: Several `DashMap` collections have no cap/eviction policy.
- Impact: High-cardinality spam can grow memory indefinitely and degrade availability.
- Remediation:
  - Use bounded caches (TTL + LRU + hard global limits).
  - Add body-size limits, per-IP controls, key-cardinality controls, and backpressure.

### H-03 High-risk Ed25519 instruction parsing in Solana verifier

- Location: `contracts/solana_anchor/src/lib.rs` (`verify_ed25519_signature`)
- Issue: Manual offset parsing lacks strict validation of full Ed25519 instruction layout/index semantics.
- Impact: Potential mismatch between what was cryptographically verified and what is parsed/accepted.
- Remediation:
  - Parse and validate the complete canonical Ed25519 instruction layout, including index fields.
  - Add malformed-input and fuzz tests with adversarial instruction payloads.

## Medium

### M-01 `execute_graph` currently returns mocked success

- Location: `src/main.rs` (`verify_graph_execution`)
- Issue: Request payload is ignored; endpoint returns fixed “success” behavior.
- Impact: Upstream systems can mistakenly trust non-verified results.
- Remediation:
  - Disable endpoint by default until real verification exists, or gate behind an explicit feature flag.
  - Bind output to validated input hash, requester identity, and nonce.

### M-02 Internal error details exposed to clients

- Location: `src/main.rs` (`verify_proof`, `format!("{:?}", e)`)
- Issue: Raw internal errors are returned in API responses.
- Impact: Information disclosure useful for reconnaissance and endpoint behavior fingerprinting.
- Remediation:
  - Return stable generic client-safe error messages.
  - Log detailed diagnostics server-side only.

### M-03 Solana TokenAccount constraints remain loose

- Location: `contracts/solana_anchor/src/lib.rs` (`CreateCampaign`, `ClaimPayout`, `CancelCampaign`)
- Issue: Critical token accounts are not fully constrained with strict `token::mint` and `token::authority` checks.
- Impact: Greater risk of misrouting/incorrect account wiring in hostile or faulty integrations.
- Remediation:
  - Add explicit account constraints for advertiser, agent, vault, and mint consistency.
  - Prefer ATA-based constraints where possible.

## Low

### L-01 Rust supply-chain lock discipline is insufficient

- Location: `Cargo.toml` (`zerolang` pinned to git branch `main`), missing repository `Cargo.lock`
- Issue: Build output can drift with upstream branch movement.
- Impact: Non-reproducible builds and increased supply-chain compromise risk.
- Remediation:
  - Pin git dependency to immutable commit (`rev`).
  - Commit `Cargo.lock` and enforce `cargo build --locked`.

### L-02 NPM manifest and lockfile baseline drift

- Location: `contracts/evm/package.json`, `contracts/evm/package-lock.json`
- Issue: Lockfile still includes stale version markers (for example `"hh2"`), inconsistent with manifest.
- Impact: Reduced reproducibility and auditability of dependency tree.
- Remediation:
  - Regenerate lockfile from clean baseline.
  - Enforce `npm ci`, lockfile integrity checks, and dependency audit in CI.

### L-03 Docker runtime hardening gaps

- Location: `Dockerfile`
- Issue: Uses `rust:latest`, runtime default root user, minimal hardening controls.
- Impact: Larger attack surface and higher impact if container compromise occurs.
- Remediation:
  - Pin base images by digest.
  - Run as non-root, minimize packages/capabilities, set read-only filesystem and resource controls.

---

## 4. Positive Security Controls Already Present

- EVM contract uses `SafeERC20`, `ReentrancyGuard`, and signature expiry checks.
- Oracle private key memory is zeroized on drop (`zeroize`).
- Oracle endpoints include API key and rate-limiting framework (needs promotion from optional to mandatory).

---

## 5. Recommended Remediation Priority

- **P0 (Immediate):** C-01, C-02, H-01, H-03
- **P1 (Current sprint):** H-02, M-01, M-02, M-03
- **P2 (Next sprint):** L-01, L-02, L-03

---

## 6. Suggested Regression Test Suite

1. Auth enforcement: process must fail startup when `API_SECRET` is required but missing.
2. Identity binding: reject signature issuance when GitHub identity and wallet ownership do not match.
3. Deadline policy: reject expired and far-future deadlines.
4. Replay resistance: one claim nonce/claim-id must be one-time consumable.
5. DoS resilience: high-cardinality spam must not produce unbounded memory growth.
6. Solana Ed25519 negative tests: malformed offsets/index/message combinations must all fail.
7. Supply-chain checks: `cargo build --locked`, `npm ci`, and container scanning (Trivy/Scout).

---

## 7. Audit Notes

- This report is a point-in-time static analysis and is not a formal proof of security.
- Before mainnet use, complete all P0/P1 fixes and obtain an independent third-party manual audit.

---

**Auditor Signature**  
GPT-5.3-Codex
