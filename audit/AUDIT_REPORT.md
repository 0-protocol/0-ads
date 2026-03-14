# 0-ads Security Audit Report

**Protocol**: 0-ads — The Agent-Native Advertising Network  
**Auditor**: Claude Opus 4.6 (Anthropic LLM)  
**Date**: 2026-03-14  
**Scope**: Full-stack audit covering EVM smart contracts, Solana Anchor program, off-chain oracle, P2P Billboard node, Python SDK, and deployment configuration  
**Severity Classification**: Critical / High / Medium / Low / Informational  

---

## Executive Summary

The 0-ads protocol implements an agent-native advertising escrow system with on-chain settlement on both EVM (Base Sepolia) and Solana. The system relies on an off-chain oracle to verify agent actions (e.g., GitHub stars) and produce ECDSA signatures that unlock escrowed funds via smart contracts.

This audit identified **5 Critical**, **7 High**, **8 Medium**, **4 Low**, and **3 Informational** findings across all components. The Solana Anchor program is **not deployment-ready** due to completely missing signature verification and double-claim protection. The EVM contract (`AdEscrow.sol`) is more mature but contains fund-locking risks and campaign overwrite vulnerabilities. The off-chain oracle and Billboard node have significant operational security gaps.

**Overall Risk Rating: HIGH**

The deployed contract at `0x8871169e040c7a840EB063AC9e3a31D44De956A2` on Base Sepolia should be treated as **testnet-only** and must not hold any mainnet value without remediation.

---

## Deployed Contract Details

| Property | Value |
|----------|-------|
| Contract | `AdEscrow.sol` |
| Address | `0x8871169e040c7a840EB063AC9e3a31D44De956A2` |
| Network | Base Sepolia L2 (Testnet) |
| Solidity | `^0.8.24` (Cancun EVM) |
| Dependencies | OpenZeppelin Contracts `^5.6.1` |
| Hardhat | `^3.1.12` |

---

## Table of Findings

| ID | Severity | Component | Title |
|----|----------|-----------|-------|
| C-01 | Critical | Solana | Missing Oracle Signature Verification in `claim_payout` |
| C-02 | Critical | Solana | No Double-Claim Protection |
| C-03 | Critical | Solana | No Budget Tracking or Decrement |
| C-04 | Critical | Billboard | Random Oracle Key on Every Startup |
| C-05 | Critical | EVM | Campaign Overwrite Allows Fund Theft |
| H-01 | High | EVM | No Campaign Cancellation or Fund Recovery |
| H-02 | High | EVM | Residual Funds Permanently Locked |
| H-03 | High | EVM | Non-Standard ERC20 Tokens Revert (Missing SafeERC20) |
| H-04 | High | Billboard | Unauthenticated Oracle API Endpoints |
| H-05 | High | Billboard | Silent Hex Parsing Failures |
| H-06 | High | Oracle | Single Key, No Rotation, No Revocation |
| H-07 | High | Solana | Missing Vault PDA Validation in ClaimPayout |
| M-01 | Medium | EVM | No Signature Expiration or Deadline |
| M-02 | Medium | EVM | No Payout-Budget Ratio Validation |
| M-03 | Medium | Oracle | Weak GitHub Star Verification |
| M-04 | Medium | Oracle | No Rate Limiting or Idempotency |
| M-05 | Medium | Billboard | Gossipsub Accepts Unvalidated Intents |
| M-06 | Medium | Billboard | Thread Pool Exhaustion via Graph Execution |
| M-07 | Medium | Solana | Unchecked `usdc_mint` Account |
| M-08 | Medium | SDK | Wallet Private Key Stored in Plaintext |
| L-01 | Low | EVM | Missing Events for Budget State Changes |
| L-02 | Low | Billboard | No Graceful Shutdown |
| L-03 | Low | Oracle | Private Key Not Zeroized on Drop |
| L-04 | Low | Genesis | Truncated Hash and Unused Import |
| I-01 | Info | EVM | Reentrancy Risk Mitigated but No Explicit Guard |
| I-02 | Info | EVM | No Test Suite Provided |
| I-03 | Info | Solana | Test Suite is Empty Skeleton |

---

## Critical Findings

### C-01: Missing Oracle Signature Verification in Solana `claim_payout`

**Component**: `contracts/solana_anchor/src/lib.rs` (line 42–68)  
**Severity**: Critical  

**Description**: The `claim_payout` function accepts an `_oracle_signature` parameter (prefixed with underscore, indicating it is unused) but **never verifies it**. The code contains only a comment suggesting that Ed25519 signature verification "would" be done via the sysvar instructions pre-compile, but no such logic exists.

```rust
pub fn claim_payout(ctx: Context<ClaimPayout>, _oracle_signature: [u8; 64]) -> Result<()> {
    let campaign = &mut ctx.accounts.campaign;
    // Comment about "would verify" — but no actual verification
    // ... proceeds directly to transfer
}
```

**Impact**: Any Solana account can drain all escrowed funds from any campaign by calling `claim_payout` with an arbitrary 64-byte array. This is a **total loss of funds** vulnerability.

**Recommendation**: Implement Ed25519 signature verification using the `ed25519_program` sysvar instruction introspection pattern:
```rust
use solana_program::sysvar::instructions;
let ix = instructions::load_instruction_at_checked(0, &ctx.accounts.instruction_sysvar)?;
// Verify ix.program_id == ed25519_program::id()
// Verify the signed message matches the expected payload
```

---

### C-02: No Double-Claim Protection (Solana)

**Component**: `contracts/solana_anchor/src/lib.rs`  
**Severity**: Critical  

**Description**: Unlike the EVM contract which maintains a `hasClaimed` mapping, the Solana program has **no mechanism** to prevent the same agent from calling `claim_payout` repeatedly for the same campaign. There is no claim receipt PDA, no boolean flag, and no state transition that would prevent re-execution.

**Impact**: A single agent can drain an entire campaign vault by calling `claim_payout` in a loop until the vault is empty.

**Recommendation**: Create a `ClaimReceipt` PDA account seeded by `[b"claimed", campaign_id, agent_pubkey]` that is initialized on first claim and checked for existence on subsequent calls.

---

### C-03: No Budget Tracking or Decrement (Solana)

**Component**: `contracts/solana_anchor/src/lib.rs`  
**Severity**: Critical  

**Description**: The `CampaignState` struct does not include a `budget` field. While the `create_campaign` function transfers `budget` tokens to the vault, the on-chain state never records or decrements the remaining budget. The `claim_payout` function always transfers `campaign.payout` tokens without checking if the vault has sufficient balance.

```rust
pub struct CampaignState {
    pub advertiser: Pubkey,
    pub campaign_id: [u8; 32],
    pub payout: u64,                    // per-claim amount
    pub verification_graph_hash: [u8; 32],
    pub oracle_pubkey: Pubkey,
    // NOTE: no `budget` or `remaining` field
}
```

**Impact**: Combined with C-01 and C-02, this makes it impossible for the contract to enforce budget limits. The vault will simply fail when the SPL token balance reaches zero, with no graceful handling.

**Recommendation**: Add a `remaining_budget: u64` field to `CampaignState`, check `remaining_budget >= payout` before transfer, and decrement atomically.

---

### C-04: Random Oracle Key Generated on Every Startup

**Component**: `src/main.rs` (lines 60–62)  
**Severity**: Critical  

**Description**: The Billboard node generates a fresh random oracle private key every time the process starts:

```rust
let mut oracle_key = [0u8; 32];
rand::thread_rng().fill_bytes(&mut oracle_key);
```

**Impact**:
1. Every node restart produces a new oracle identity. Any previously issued signatures become invalid because the on-chain campaign references the old oracle address.
2. If a campaign is created with oracle address A, and the node restarts (getting new key B), all pending agent claims for that campaign will fail permanently.
3. There is no key persistence, no seed phrase, and no HSM integration.

**Recommendation**: Load the oracle private key from a secure configuration source (environment variable, encrypted keyfile, or hardware security module). Never generate it randomly at runtime in production.

---

### C-05: Campaign Overwrite Allows Fund Theft

**Component**: `contracts/evm/contracts/AdEscrow.sol` (line 28–48)  
**Severity**: Critical  

**Description**: `createCampaign` does not check whether a campaign with the given `campaignId` already exists. An attacker can overwrite an existing funded campaign by calling `createCampaign` with the same `campaignId`, a different `oracle` address (one they control), and a minimal `budget` (e.g., 1 wei of a worthless token).

```solidity
function createCampaign(
    bytes32 campaignId,
    IERC20 token,
    uint256 budget,
    ...
    address oracle
) external {
    // No existence check — overwrites silently
    require(token.transferFrom(msg.sender, address(this), budget), "Transfer failed");
    campaigns[campaignId] = Campaign({ ... });
}
```

**Impact**: The overwrite replaces the `oracle` field with the attacker's oracle. The original budget (e.g., 10,000 USDC) is still in the contract but now governed by the attacker's oracle. The attacker signs their own payout claims and drains the original advertiser's funds.

**Recommendation**: Add an existence check:
```solidity
require(campaigns[campaignId].advertiser == address(0), "Campaign already exists");
```

---

## High Severity Findings

### H-01: No Campaign Cancellation or Fund Recovery (EVM)

**Component**: `contracts/evm/contracts/AdEscrow.sol`  
**Severity**: High  

**Description**: The contract provides no mechanism for an advertiser to cancel a campaign and withdraw remaining budget. Once funds are deposited via `createCampaign`, they can only leave the contract through `claimPayout`. If a campaign receives no valid claims (e.g., oracle goes offline, campaign is unpopular), the funds are permanently locked.

**Recommendation**: Add a `cancelCampaign(bytes32 campaignId)` function restricted to the original advertiser, with a timelock to prevent front-running of pending claims.

---

### H-02: Residual Funds Permanently Locked (EVM)

**Component**: `contracts/evm/contracts/AdEscrow.sol` (line 55, 75)  
**Severity**: High  

**Description**: When `budget` is not evenly divisible by `payout`, the remainder is permanently trapped. Example: budget = 100 USDC, payout = 30 USDC. After 3 claims (90 USDC paid), 10 USDC remains. The 4th claim requires `c.budget >= c.payout` (10 >= 30) which fails. The 10 USDC is irrecoverable.

**Recommendation**: Implement a withdrawal function for the advertiser to reclaim residual funds after the campaign ends or a deadline passes.

---

### H-03: Non-Standard ERC20 Tokens Will Revert (EVM)

**Component**: `contracts/evm/contracts/AdEscrow.sol` (lines 36, 76)  
**Severity**: High  

**Description**: The contract uses bare `IERC20.transferFrom()` and `IERC20.transfer()` wrapped in `require()`:

```solidity
require(token.transferFrom(msg.sender, address(this), budget), "Transfer failed");
require(c.token.transfer(msg.sender, c.payout), "Transfer failed");
```

Non-standard ERC20 tokens (notably USDT on Ethereum, and potentially tokens on Base) do not return a `bool`. The ABI decoder will revert when attempting to decode the missing return value.

**Recommendation**: Use OpenZeppelin's `SafeERC20` library with `safeTransferFrom` and `safeTransfer`, which handles both standard and non-standard return values.

---

### H-04: Unauthenticated Oracle API Endpoints

**Component**: `src/main.rs` (lines 73–79)  
**Severity**: High  

**Description**: All HTTP API endpoints — including `/api/v1/oracle/verify` and `/api/v1/oracle/execute_graph` — are publicly accessible with no authentication. Any external party can:
- Request oracle signatures for arbitrary campaigns.
- Broadcast fake intents to the network.
- Trigger expensive graph execution.

**Recommendation**: Implement API key authentication, request signing, or mutual TLS for oracle endpoints. At minimum, rate-limit the oracle verification endpoint per IP.

---

### H-05: Silent Hex Parsing Failures

**Component**: `src/main.rs` (lines 124–138)  
**Severity**: High  

**Description**: The `hex_to_32` and `hex_to_20` utility functions use `unwrap_or_default()`:

```rust
let bytes = hex::decode(s.trim_start_matches("0x")).unwrap_or_default();
```

If a caller provides malformed hex (e.g., `"0xGGGG"`), the function silently returns a zero-filled array instead of returning an error. This zero array is then used in the oracle signature payload, producing a valid signature for the **wrong** parameters.

**Impact**: An oracle signature generated with incorrect parameters will not match on-chain, causing legitimate claims to fail silently. Worse, if an attacker deliberately provides a zero-filled campaign ID that matches an existing zero-ID campaign, the signature could be used to steal funds.

**Recommendation**: Return `Result<[u8; N], Error>` and propagate parsing errors to the API response.

---

### H-06: Single Oracle Key, No Rotation, No Revocation

**Component**: `src/oracle.rs`  
**Severity**: High  

**Description**: The oracle operates with a single ECDSA private key. There is no mechanism for:
- Key rotation (replacing a compromised key)
- Multi-sig oracle consensus
- Signature revocation (invalidating a mistakenly issued signature)

If the oracle key is compromised, every campaign referencing that oracle is immediately drainable.

**Recommendation**: Implement a multi-oracle threshold signature scheme, or at minimum, support key rotation with on-chain oracle address updates by the campaign advertiser.

---

### H-07: Missing Vault PDA Validation in ClaimPayout (Solana)

**Component**: `contracts/solana_anchor/src/lib.rs` (lines 95–106)  
**Severity**: High  

**Description**: The `ClaimPayout` account context does not constrain `vault_token_account` to be the PDA derived from the campaign's `campaign_id`:

```rust
pub struct ClaimPayout<'info> {
    #[account(mut)]
    pub campaign: Account<'info, CampaignState>,
    #[account(mut)]
    pub vault_token_account: Account<'info, TokenAccount>,  // No seeds constraint
    ...
}
```

In `CreateCampaign`, the vault is derived as `seeds = [b"vault", campaign_id.as_ref()]`. But in `ClaimPayout`, any `TokenAccount` can be passed.

**Impact**: An attacker could substitute a vault that doesn't belong to the campaign, potentially redirecting funds from a different vault or causing unexpected behavior.

**Recommendation**: Add PDA seeds and bump constraint:
```rust
#[account(
    mut,
    seeds = [b"vault", campaign.campaign_id.as_ref()],
    bump,
)]
pub vault_token_account: Account<'info, TokenAccount>,
```

---

## Medium Severity Findings

### M-01: No Signature Expiration or Deadline (EVM)

**Component**: `contracts/evm/contracts/AdEscrow.sol` (lines 59–65)  
**Severity**: Medium  

**Description**: The oracle signature payload includes `chainid`, `contract address`, `campaignId`, `msg.sender`, and `payout` — but no timestamp, deadline, or nonce. Once signed, a payout authorization is valid indefinitely as long as the campaign has budget.

**Impact**: If the oracle mistakenly signs a proof (false positive verification), there is no way to expire or invalidate the signature.

**Recommendation**: Add a `deadline` field (block timestamp) to the signed payload and enforce `require(block.timestamp <= deadline)` in `claimPayout`.

---

### M-02: No Payout-Budget Ratio Validation (EVM)

**Component**: `contracts/evm/contracts/AdEscrow.sol`  
**Severity**: Medium  

**Description**: `createCampaign` does not validate that:
- `payout > 0`
- `budget >= payout`
- `budget` is a reasonable multiple of `payout`

A campaign with `payout = 0` passes all checks in `claimPayout` and executes a zero-value transfer (wasting gas). A campaign with `budget < payout` will lock the budget permanently (no claims possible, no withdrawal).

**Recommendation**: Add `require(payout > 0 && budget >= payout)` in `createCampaign`.

---

### M-03: Weak GitHub Star Verification (Oracle)

**Component**: `src/oracle.rs` (lines 33–58)  
**Severity**: Medium  

**Description**: The oracle verifies GitHub stars by making a GET request to `https://api.github.com/users/{id}/starred/{repo}`. This approach has several weaknesses:
1. **Time-of-Check/Time-of-Use (TOCTOU)**: An agent can star a repo, get the oracle signature, then immediately un-star.
2. **API Endpoint Accuracy**: The GitHub API endpoint for checking if a user starred a specific repo is `GET /user/starred/{owner}/{repo}` (authenticated, checking the authenticated user) or checking the stargazers list. The endpoint used may not function as expected.
3. **Rate Limits**: GitHub API rate limits (60 requests/hour unauthenticated) can be easily exhausted.

**Recommendation**: Use GitHub webhooks or authenticated API calls with proper endpoint validation. Consider implementing a cooldown period before signing.

---

### M-04: No Rate Limiting or Idempotency (Oracle)

**Component**: `src/oracle.rs`  
**Severity**: Medium  

**Description**: The oracle will issue signatures for the same agent+campaign pair unlimited times. While the EVM contract's `hasClaimed` mapping prevents double-claiming, the oracle performs expensive API calls and cryptographic signing for every request without deduplication.

**Impact**: An attacker can DoS the oracle by flooding it with duplicate verification requests, exhausting GitHub API rate limits and compute resources.

**Recommendation**: Maintain a signed-payloads cache keyed by `(campaignId, agentAddr)`. Return cached signatures for duplicate requests.

---

### M-05: Gossipsub Accepts Unvalidated Intents (Billboard)

**Component**: `src/main.rs` (lines 90–104)  
**Severity**: Medium  

**Description**: The Billboard node accepts any JSON-deserializable `AdIntent` from the gossipsub network and inserts it into the `active_intents` DashMap without validation:

```rust
if let Ok(intent) = serde_json::from_slice::<AdIntent>(&message.data) {
    state.active_intents.insert(intent.campaign_id.clone(), intent);
}
```

**Impact**: A malicious peer can flood the network with fake campaigns (non-existent on-chain), causing agents to waste compute attempting to execute them. Fake campaigns with the same `campaign_id` as legitimate ones can overwrite real intents in the local cache.

**Recommendation**: Validate that the campaign exists on-chain before caching. Use a separate "unverified" queue and verify asynchronously.

---

### M-06: Thread Pool Exhaustion via Graph Execution (Billboard)

**Component**: `src/main.rs` (lines 168–191)  
**Severity**: Medium  

**Description**: The `/api/v1/oracle/execute_graph` endpoint spawns a blocking task for each request via `tokio::task::spawn_blocking`. Without rate limiting, an attacker can submit thousands of graph execution requests, exhausting Tokio's blocking thread pool and starving all other blocking operations.

**Recommendation**: Implement a semaphore or bounded channel to limit concurrent graph executions. Apply per-IP rate limiting on the endpoint.

---

### M-07: Unchecked `usdc_mint` Account (Solana)

**Component**: `contracts/solana_anchor/src/lib.rs` (lines 89–90)  
**Severity**: Medium  

**Description**: The `usdc_mint` field in `CreateCampaign` uses `/// CHECK: Safe` without any actual validation:

```rust
/// CHECK: Safe
pub usdc_mint: AccountInfo<'info>,
```

This allows campaigns to be created with arbitrary token mints, not just USDC. While this might be intentional for flexibility, the comment "Safe" is misleading and provides no justification.

**Recommendation**: Either validate against a known USDC mint address, or rename the field and document that arbitrary tokens are supported.

---

### M-08: Wallet Private Key Stored in Plaintext (SDK)

**Component**: `python/zero_ads_sdk/client.py` (line 12)  
**Severity**: Medium  

**Description**: The `ZeroAdsClient` constructor accepts and stores a wallet private key as a plaintext string. The key is also partially logged (line 54: `self.wallet_key[:6]`).

**Recommendation**: Accept a signer interface or keystore path instead of raw private key material. Never log any portion of a private key.

---

## Low Severity Findings

### L-01: Missing Events for Budget State Changes (EVM)

**Description**: The contract emits `CampaignCreated` and `PayoutClaimed` but does not emit events when a campaign's budget is exhausted or when the budget changes. Off-chain indexers cannot efficiently track campaign status.

---

### L-02: No Graceful Shutdown (Billboard)

**Description**: The `server_handle` tokio task is spawned but never awaited. The main loop runs indefinitely with no SIGTERM/SIGINT handling. Pending operations may be lost on shutdown.

---

### L-03: Private Key Not Zeroized on Drop (Oracle)

**Description**: The `oracle_private_key: [u8; 32]` field in `AttentionOracle` is not zeroized when the struct is dropped. The key material remains in memory and could be read via memory dumps or swap.

**Recommendation**: Use `zeroize::Zeroize` trait on the key material.

---

### L-04: Truncated Hash and Unused Import (Genesis)

**Description**: In `scripts/genesis.py`, the `verificationGraphHash` is set to a truncated placeholder `"0x4f8b9e...2a1"`. The `secp256k1` module is imported but never used.

---

## Informational Findings

### I-01: Reentrancy Mitigated but No Explicit Guard (EVM)

**Description**: In `claimPayout`, state updates (`hasClaimed` and `budget` decrement) occur before the external `token.transfer` call, following the Checks-Effects-Interactions pattern. This is correct. However, an explicit `ReentrancyGuard` (OpenZeppelin) would provide defense-in-depth against future refactoring mistakes.

---

### I-02: No Test Suite for EVM Contract

**Description**: The `package.json` test script is `echo "Error: no test specified" && exit 1`. No Hardhat tests exist for `AdEscrow.sol`. Formal verification or at minimum comprehensive unit tests are strongly recommended before any mainnet deployment.

---

### I-03: Empty Test Skeleton for Solana Program

**Description**: The Solana test file (`tests/zero_ads.ts`) contains only a `console.log` statement and no actual test logic.

---

## Recommendations Summary

### Immediate (Pre-Mainnet, Blocking)

1. **EVM**: Add campaign existence check in `createCampaign` to prevent overwrites (C-05)
2. **EVM**: Implement `SafeERC20` for all token transfers (H-03)
3. **EVM**: Add `cancelCampaign` / `withdrawRemainder` for advertisers (H-01, H-02)
4. **EVM**: Add comprehensive Hardhat test suite with edge cases
5. **Solana**: Implement Ed25519 signature verification in `claim_payout` (C-01)
6. **Solana**: Add double-claim PDA protection (C-02)
7. **Solana**: Add `remaining_budget` field and decrement logic (C-03)
8. **Solana**: Add vault PDA constraint in `ClaimPayout` (H-07)
9. **Billboard**: Load oracle key from persistent secure storage (C-04)

### Short-Term (Pre-Production)

10. **Billboard**: Add API authentication and rate limiting (H-04)
11. **Oracle**: Implement signature caching and rate limiting (M-04)
12. **Oracle**: Improve GitHub verification with authenticated API calls (M-03)
13. **EVM**: Add signature deadline/expiration (M-01)
14. **EVM**: Add payout/budget validation (M-02)
15. **Billboard**: Validate gossipsub intents against on-chain state (M-05)

### Long-Term (Hardening)

16. Multi-oracle threshold signature scheme (H-06)
17. Formal verification of EVM contract invariants
18. zkTLS integration for trustless Web2 verification
19. Bug bounty program post-mainnet deployment

---

## Appendix A: Files Reviewed

| File | Lines | Language |
|------|-------|----------|
| `contracts/evm/contracts/AdEscrow.sol` | 80 | Solidity |
| `contracts/evm/scripts/deploy.js` | 13 | JavaScript |
| `contracts/evm/hardhat.config.js` | 19 | JavaScript |
| `contracts/evm/package.json` | 18 | JSON |
| `contracts/solana_anchor/src/lib.rs` | 116 | Rust |
| `contracts/solana_anchor/tests/zero_ads.ts` | 11 | TypeScript |
| `src/main.rs` | 191 | Rust |
| `src/oracle.rs` | 121 | Rust |
| `src/network.rs` | 36 | Rust |
| `src/lib.rs` | 2 | Rust |
| `python/zero_ads_sdk/client.py` | 55 | Python |
| `scripts/genesis.py` | 44 | Python |
| `schema/0-ads.capnp` | 27 | Cap'n Proto |
| `Cargo.toml` | 25 | TOML |
| `README.md` | 75 | Markdown |
| `ROADMAP.md` | 63 | Markdown |

**Total Lines Reviewed**: ~896

---

## Appendix B: Methodology

This audit was conducted through manual static analysis of all source code in the repository. The following techniques were employed:

1. **Smart Contract Analysis**: Line-by-line review of Solidity and Anchor/Rust contracts for common vulnerability patterns (reentrancy, access control, integer overflow, signature replay, fund locking).
2. **Oracle Security Review**: Analysis of the off-chain signing logic for cryptographic correctness, domain separation, and replay resistance.
3. **Infrastructure Review**: Assessment of the P2P network layer, HTTP API surface, and operational security posture.
4. **SDK Review**: Evaluation of client-side key management and data handling practices.
5. **Cross-Component Analysis**: Verification that the EVM contract's signature scheme matches the oracle's signing implementation (ABI encoding layout, EIP-191 prefix, recovery ID handling).

No dynamic testing, fuzzing, or formal verification was performed. No on-chain transaction analysis of the deployed Base Sepolia contract was conducted.

---

*This report is provided for informational purposes. It represents a best-effort analysis at a single point in time and does not constitute a guarantee of security. Smart contract interactions carry inherent risk.*

**Auditor**: Claude Opus 4.6 — Anthropic Large Language Model  
**Report Version**: 1.0  
**Commit**: HEAD of `main` branch as of 2026-03-14  
