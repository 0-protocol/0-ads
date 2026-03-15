# 0-ads Security Audit Remediation Plan

## Audit Sources
1. `audit/Full_System_Audit_Report.md` — OpenZeppelin + SlowMist joint audit
2. `audit/AdEscrow_Audit_Report.md` — AdEscrow-specific contract audit
3. `audit/AUDIT_REPORT.md` — Claude Opus 4.6 V3 final audit
4. `audit/AUDIT_REPORT_GPT-5.3-CODEX.md` — GPT-5.3-Codex second re-audit

## P0 — Critical / Blockers

- [x] **EVM: Fee-on-transfer token handling** (Full_System 4.2, AdEscrow 3.1)
  - Record balance before/after `safeTransferFrom` in `createCampaign`
  - Use actual received amount as budget
- [x] **Oracle: Signature cache key flaw + TTL** (Full_System 3.4)
  - Include `deadline` and `payout` in cache key
  - Add TTL-based expiry to prevent stale cached signatures
- [x] **Rust: broadcast_intent missing gossipsub publish** (Full_System 3.2)
  - Add `mpsc::channel` between HTTP handler and swarm event loop
  - Actually publish intents to P2P network
- [x] ~~**Solana: Cross-deployment replay** (Full_System 4.1)~~ — N/A, Solana sunset

## P1 — High

- [x] **EVM: Oracle update race condition / rug pull** (AdEscrow 3.3, Full_System 4.2)
  - Add oracle update grace period (1hr) with `previousOracle` + `oracleUpdatedAt`
- [x] ~~**Oracle: Multi-chain signing** (Full_System 2.1)~~ — N/A, Solana sunset
- [x] **EVM: Explicit campaign existence check** (AdEscrow 2.2)
  - Add `require(c.advertiser != address(0))` in claimPayout

## P2 — Medium

- [x] **Rust: Anonymous rate limit key** (Full_System 3.3)
  - Use per-agent/per-IP identifier instead of shared "anonymous"
- [x] ~~**Solana: Oracle update sentinel check** (GPT-5.3-CODEX R-04)~~ — N/A, Solana sunset

## V3 Audit Remediation (Update_Audit_Review_V3.md)

- [x] **EVM: Gasless relayer incompatible with claimPayout** (CertiK — Critical)
  - Added `claimPayoutFor(campaignId, agent, deadline, oracleSignature)` delegated entrypoint
  - `claimPayout` is now a thin wrapper calling `_claimPayoutFor(msg.sender)`
  - Relayer updated to use `claimPayoutFor` with agent address on-chain
  - SDK / MCP / examples updated to pass agent address through relay flow
  - 7 new regression tests covering delegated claims, fund-redirect prevention, cross-path replay
- [x] **Backend: Mock signatures and fail-open verifier branches** (SlowMist — High)
  - Removed `0xUniversalSignedProofOfIntent...` mock from `universal_oracle.py`
  - Twitter and Xiaohongshu verifiers now fail-closed when credentials are missing
  - Anti-sybil endpoint no longer returns mock `0x...` signatures
  - Both backend modules marked as prototype with clear documentation headers
- [x] **Backend: Weak anti-sybil heuristics not enforced** (SlowMist — High)
  - Added pluggable `SybilPolicy` to Rust oracle (`src/oracle.rs`)
  - Enforced in `verify_proof` flow — signatures not issued until policy passes
  - Configurable via `SYBIL_POLICY`, `SYBIL_MIN_AGE_DAYS`, `SYBIL_MIN_FOLLOWERS`, `SYBIL_MIN_REPOS`
  - Fail-closed: GitHub API errors cause rejection
- [x] **Ops: Centralized pause via single EOA** (OpenZeppelin — Medium)
  - Deploy script updated with `SAFE_ADDRESS` env for immediate ownership transfer
  - Documentation added for multisig handoff and pause governance
- [x] **Ops: Default oracle key allows accidental dev-key usage** (New finding)
  - Removed hardcoded fallback key from `load_oracle_key()`
  - Node now fails fast if neither `ORACLE_PRIVATE_KEY` nor `ORACLE_KEY_FILE` is set

## Review
- [x] All 33 EVM tests passing (26 original + 7 new delegated claim tests)
- [x] V4 Remediation Report written to `audit/V4_Remediation_Report.md`
- [x] CI audit guard script at `scripts/audit_guard.sh` — all checks pass

---

## V5 Blackhat Audit Remediation (BLACKHAT_AUDIT_REPORT.md)

### P0 — Critical (Before Mainnet)

- [x] **BH-C01: Oracle substring match exploit** — Replaced `body.contains()` with proper JSON parsing and exact `full_name` match. Added pagination through all GitHub starred repo pages.
- [x] **BH-C02: MCP private key leak** — Removed ephemeral key from MCP response. Added persistent encrypted wallet keyfile with optional auto-sweep to user-provided safe address.
- [x] **BH-C03: Cache key missing chain_id/contract_addr** — Added `chain_id` and `contract_addr` to `CacheKey` struct.
- [x] **BH-H05: No on-chain max deadline** — Added `MAX_DEADLINE_WINDOW = 2 hours` require in `_claimPayoutFor`.

### P1 — High (Before Mainnet)

- [x] **BH-H03: Relayer no auth + nonce race** — Added API key auth, asyncio-based nonce manager, IP rate limiter, proper HTTP error codes.
- [x] **BH-H02: Intent queue mass flush DoS** — Replaced atomic `clear()` with LRU eviction of overflow entries.
- [x] **BH-M03: Broadcast endpoint no auth** — Added `check_api_key` and routes through `unverified_intents` queue.
- [x] **BH-M04: No TLS** — Added TLS support via `axum-server` with `tls-rustls` feature, configurable via `TLS_CERT_PATH`/`TLS_KEY_PATH`.

### P2 — Medium (Post Launch)

- [x] **BH-H01: Static wallet bind** — Added timestamp to challenge message with 10-minute max age.
- [x] **BH-H04: Fee-on-transfer residual** — Added `sweepDust()` function to `AdEscrow.sol`.
- [x] **BH-M01: Rate limit header spoofing** — Removed trust in `x-forwarded-for`/`x-real-ip`; uses API key or agent ID as rate key.
- [x] **BH-M02: Campaign ID squatting** — Added `deriveCampaignId()` for sender-scoped deterministic IDs.
- [x] **BH-M05: No P2P peer discovery** — Added persistent node identity, bootstrap peer dialing via `BOOTSTRAP_PEERS` env var.
- [x] **BH-M06: cancelCampaign not pausable** — Added `whenNotPaused` to `cancelCampaign` and `updateOracle`.
- [x] **BH-M07: Relayer 200 on error** — Replaced `return {"error": ...}` with proper `HTTPException` status codes.
- [x] **BH-L02: SDK non-functional for mainnet** — Implemented real oracle + relayer claim flow in `submit_proof_and_claim`.
- [x] **BH-L03: Oracle key env var warning** — Added warning log when `ORACLE_PRIVATE_KEY` env var is used.

---

## V2 Re-Audit Remediation (NEW-1 through NEW-8)

- [x] **NEW-1: MCP default wallet password** — Replaced hardcoded password with machine-derived entropy (`uuid.getnode()` + `getpass.getuser()` + keystore path). Loud warning when no explicit password set.
- [x] **NEW-2: Random intent eviction** — Added FIFO insertion-order tracking via `VecDeque<String>` alongside `DashMap`. Eviction now pops from front of queue instead of arbitrary `DashMap` iteration.
- [x] **NEW-3: Relayer auth disabled by default** — Changed default to `RELAYER_AUTH_REQUIRED=true`. Empty `RELAYER_API_KEYS` now returns 503. Added `TRUSTED_PROXY_IPS` for safe `x-forwarded-for` parsing.
- [x] **NEW-4: Campaign ID squatting not enforced** — Removed `campaignId` parameter from `createCampaign`. IDs are now internally derived from `keccak256(msg.sender, nonce)`. Per-sender nonces via `campaignNonces` mapping.
- [x] **NEW-5: TLS silent fallback** — TLS cert load failure now calls `std::process::exit(1)` unless `ALLOW_TLS_FALLBACK=true` is set.
- [x] **NEW-6: sweepDust missing whenNotPaused** — Added `whenNotPaused` modifier to `sweepDust`.
- [x] **NEW-7: deriveCampaignId UX** — Replaced state-changing `deriveCampaignId()` with `view` function `previewCampaignId(address sender)`. Nonce consumed only inside `createCampaign`.
- [x] **NEW-8: No test coverage for V5 changes** — Added 47+ tests covering `sweepDust`, `previewCampaignId`, `MAX_DEADLINE_WINDOW`, pause guards for all functions, and adapted all existing tests to new `createCampaign` signature.

---

## V3 Final Audit Close (BLACKHAT_AUDIT_REPORT.md)

- [x] Final close status recorded: overall risk **LOW**, 29/31 findings fully resolved.
- [x] Residual observations addressed in final perfection pass:
  - Stale `unverified_order` keys compacted periodically in `src/main.rs`.
  - mDNS peer discovery added to P2P behavior and swarm event handling.
  - Deploy script aligned with current `createCampaign(token, budget, payout, graphHash, oracle)` signature and event-based campaign ID extraction.

## Phase 3 Roadmap (Next Milestones)

- [ ] **DON (Decentralized Oracle Network)** — threshold signatures (t-of-n) and key-rotation governance.
- [ ] **ZK-TLS Verification Path** — trust-minimized off-chain proof ingestion for social actions.
- [ ] **UUPS Upgradeability Track** — migration plan, storage layout checks, and governance guardrails.
- [ ] **Dispute / Fraud-Proof Mechanism** — challenge windows, slashing policy, and evidence format.
- [ ] **Subgraph + Monitoring** — realtime indexer, anomaly alerts, and operator dashboards.
