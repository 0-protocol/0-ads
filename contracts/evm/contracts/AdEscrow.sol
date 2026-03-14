// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";

/// Phase 4: AdEscrow - Atomic Settlement for Agent-Native Ads
contract AdEscrow {
    using ECDSA for bytes32;

    struct Campaign {
        address advertiser;
        IERC20 token;
        uint256 budget;
        uint256 payout;
        bytes32 verificationGraphHash;
        address oracle;
    }

    mapping(bytes32 => Campaign) public campaigns;
    mapping(bytes32 => mapping(address => bool)) public hasClaimed;

    event CampaignCreated(bytes32 indexed campaignId, address indexed advertiser, uint256 payout);
    event PayoutClaimed(bytes32 indexed campaignId, address indexed agent, uint256 amount);

    function createCampaign(
        bytes32 campaignId,
        IERC20 token,
        uint256 budget,
        uint256 payout,
        bytes32 verificationGraphHash,
        address oracle
    ) external {
        require(token.transferFrom(msg.sender, address(this), budget), "Transfer failed");
        
        campaigns[campaignId] = Campaign({
            advertiser: msg.sender,
            token: token,
            budget: budget,
            payout: payout,
            verificationGraphHash: verificationGraphHash,
            oracle: oracle
        });

        emit CampaignCreated(campaignId, msg.sender, payout);
    }

    function claimPayout(
        bytes32 campaignId,
        bytes memory oracleSignature // Must be signed by the Campaign's Oracle
    ) external {
        Campaign storage c = campaigns[campaignId];
        require(c.budget >= c.payout, "Campaign empty");
        require(!hasClaimed[campaignId][msg.sender], "Agent already claimed");

        // Verify that the Oracle signed that this Agent successfully completed the action
        // Hash: keccak256(abi.encodePacked(campaignId, msg.sender, c.verificationGraphHash, uint8(1)))
        bytes32 payloadHash = keccak256(abi.encodePacked(campaignId, msg.sender, c.verificationGraphHash, uint8(1)));
        address signer = payloadHash.toEthSignedMessageHash().recover(oracleSignature);
        
        require(signer == c.oracle, "Invalid Oracle Signature");

        // Atomically settle
        hasClaimed[campaignId][msg.sender] = true;
        c.budget -= c.payout;
        require(c.token.transfer(msg.sender, c.payout), "Transfer failed");

        emit PayoutClaimed(campaignId, msg.sender, c.payout);
    }
}
