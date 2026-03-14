// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/// Phase 4: AdEscrow - Atomic Settlement for Agent-Native Ads
contract AdEscrow is ReentrancyGuard {
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;
    using SafeERC20 for IERC20;

    struct Campaign {
        address advertiser;
        IERC20 token;
        uint256 budget;
        uint256 payout;
        bytes32 verificationGraphHash;
        address oracle;
        uint256 createdAt;
    }

    uint256 public constant CANCEL_COOLDOWN = 7 days;

    mapping(bytes32 => Campaign) public campaigns;
    mapping(bytes32 => mapping(address => bool)) public hasClaimed;

    event CampaignCreated(bytes32 indexed campaignId, address indexed advertiser, uint256 budget, uint256 payout);
    event PayoutClaimed(bytes32 indexed campaignId, address indexed agent, uint256 amount);
    event CampaignCancelled(bytes32 indexed campaignId, address indexed advertiser, uint256 refund);
    event CampaignExhausted(bytes32 indexed campaignId);

    function createCampaign(
        bytes32 campaignId,
        IERC20 token,
        uint256 budget,
        uint256 payout,
        bytes32 verificationGraphHash,
        address oracle
    ) external {
        require(campaigns[campaignId].advertiser == address(0), "Campaign already exists");
        require(payout > 0, "Payout must be positive");
        require(budget >= payout, "Budget must cover at least one payout");

        token.safeTransferFrom(msg.sender, address(this), budget);

        campaigns[campaignId] = Campaign({
            advertiser: msg.sender,
            token: token,
            budget: budget,
            payout: payout,
            verificationGraphHash: verificationGraphHash,
            oracle: oracle,
            createdAt: block.timestamp
        });

        emit CampaignCreated(campaignId, msg.sender, budget, payout);
    }

    function claimPayout(
        bytes32 campaignId,
        uint256 deadline,
        bytes memory oracleSignature
    ) external nonReentrant {
        require(block.timestamp <= deadline, "Signature expired");

        Campaign storage c = campaigns[campaignId];
        require(c.budget >= c.payout, "Campaign empty");
        require(!hasClaimed[campaignId][msg.sender], "Agent already claimed");

        bytes32 payloadHash = keccak256(abi.encode(
            block.chainid,
            address(this),
            campaignId,
            msg.sender,
            c.payout,
            deadline
        ));

        bytes32 ethSignedMessageHash = payloadHash.toEthSignedMessageHash();
        address signer = ethSignedMessageHash.recover(oracleSignature);

        require(signer == c.oracle, "Invalid Oracle Signature");

        hasClaimed[campaignId][msg.sender] = true;
        c.budget -= c.payout;

        c.token.safeTransfer(msg.sender, c.payout);

        emit PayoutClaimed(campaignId, msg.sender, c.payout);

        if (c.budget < c.payout) {
            emit CampaignExhausted(campaignId);
        }
    }

    function cancelCampaign(bytes32 campaignId) external nonReentrant {
        Campaign storage c = campaigns[campaignId];
        require(c.advertiser == msg.sender, "Only advertiser can cancel");
        require(c.budget > 0, "No funds to withdraw");
        require(
            block.timestamp >= c.createdAt + CANCEL_COOLDOWN,
            "Cancel cooldown not elapsed"
        );

        uint256 refund = c.budget;
        c.budget = 0;

        c.token.safeTransfer(msg.sender, refund);

        emit CampaignCancelled(campaignId, msg.sender, refund);
    }
}
