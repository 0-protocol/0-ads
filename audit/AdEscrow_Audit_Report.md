# 0-ads Smart Contract Joint Audit Report

**Auditors:**
- OpenZeppelin Senior Architect (Focus on contract architecture, standards compliance, and best practices)
- SlowMist Security Engineer (Focus on threat modeling, edge cases, and attack vectors)

**Audit Target:** `0-ads/contracts/evm/contracts/AdEscrow.sol`
**Audit Date:** March 14, 2026

---

## 1. Audit Overview

The `AdEscrow` contract implements a decentralized ad agency escrow system based on atomic settlement. Advertisers can create campaigns and deposit budgets. A designated Oracle verifies the Agent's advertising behavior and signs it. Agents use this signature to claim their payout. The overall logic is clear, and OpenZeppelin standard libraries are used to ensure baseline security.

---

## 2. Code Architecture & Standards Review (OpenZeppelin Perspective)

### 2.1 Strengths
- **Correct Use of Standard Libraries:** The contract correctly imports `IERC20`, `SafeERC20`, `ECDSA`, `MessageHashUtils`, and `ReentrancyGuard`. This greatly reduces the risk of underlying implementation errors.
- **Reentrancy Protection:** The `nonReentrant` modifier is used in critical fund-outflow functions (`claimPayout` and `cancelCampaign`), effectively preventing reentrancy attacks.
- **Anti-Replay Signature Design (EIP-712 Prototype):** The construction of `payloadHash` includes `block.chainid`, `address(this)`, `campaignId`, `msg.sender`, `c.payout`, and `deadline`. This rigorous design effectively prevents cross-chain, cross-contract, and intra-campaign replay attacks.
- **Storage Packing:** The `Campaign` struct design is reasonable, though fine-tuning variable types (e.g., `uint128` or `uint96`) could further optimize Gas consumption.

### 2.2 Architecture Improvements
- **Lack of Explicit Existence Check:** In `claimPayout`, if a non-existent `campaignId` is passed, it implicitly reverts because `ECDSA.recover` does not return `address(0)`, causing `signer == c.oracle (address(0))` to fail. However, this violates the "Fail Early, Fail Loud" best practice. It is recommended to explicitly add `require(c.advertiser != address(0), "Campaign does not exist");` at the beginning of the function.
- **Event Design Refinement:** The `PayoutClaimed` event records `amount` but lacks the `deadline` or the hash of the `oracleSignature`, which might lead to a lack of context during off-chain data reconciliation.

---

## 3. Security Vulnerabilities & Threat Modeling (SlowMist Perspective)

### 3.1 [High] Compatibility Risk: Unhandled Fee-on-Transfer Tokens
- **Vulnerability Description:** In `createCampaign`, the contract uses `token.safeTransferFrom(msg.sender, address(this), budget);` and directly records `c.budget` as `budget`. If a fee-on-transfer token (like certain deflationary Meme coins) is passed, the actual amount received by the contract will be less than `budget`.
- **Impact:** This causes the recorded `budget` to be greater than the actual balance. When multiple campaigns share the same token, subsequent Agent withdrawals or Advertiser cancellations will revert due to insufficient contract balance (funds locked).
- **Remediation:** Record the balance difference before and after the transfer, or explicitly state in the documentation that fee-on-transfer tokens are not supported.

### 3.2 [Medium] Front-running & Campaign ID Hijacking (Griefing)
- **Vulnerability Description:** `createCampaign` allows the caller to customize `campaignId`. Malicious users can monitor the Mempool and front-run the transaction using the same `campaignId` when a valuable campaign is about to be created.
- **Impact:** Causes legitimate advertisers to fail in creating campaigns (Griefing Attack). If off-chain systems strongly rely on specific `campaignId`s, this will cause severe business disruption.
- **Remediation:** Auto-generate an incrementing `campaignId` internally, or generate a unique ID using a hash: `keccak256(abi.encodePacked(msg.sender, block.timestamp, nonce))`.

### 3.3 [Medium] Oracle Update Trust Crisis (Race Condition)
- **Vulnerability Description:** `updateOracle` allows advertisers to change the oracle address at any time.
- **Impact:** If an Agent has completed the task and obtained a signature from the old oracle, a malicious advertiser can front-run the `claimPayout` transaction with a higher Gas fee to call `updateOracle`, changing the oracle to an address they control. This invalidates the Agent's signature, denying them payment (Rug Pull risk).
- **Remediation:** Introduce a timelock for oracle updates, or allow a grace period in `claimPayout` where the old oracle's signature remains valid.

### 3.4 [Low] Edge Case in Campaign Exhausted Event Trigger
- **Vulnerability Description:** In `claimPayout`, the condition to trigger `CampaignExhausted` is `if (c.budget < c.payout)`.
- **Logic Analysis:** If `c.budget` exactly equals `c.payout`, it becomes 0 after deduction. Since 0 is not less than `c.payout` (assuming `c.payout > 0`), the event will not trigger as expected.
- **Remediation:** Change to `if (c.budget == 0)` or evaluate `if (c.budget < c.payout)` before the deduction.