// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/// Phase 4: AdEscrow - Atomic Settlement for Agent-Native Ads
import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/Pausable.sol";

contract AdEscrow is ReentrancyGuard, Ownable, Pausable {
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;
    using SafeERC20 for IERC20;

    constructor() Ownable(msg.sender) {}

    /**
     * @dev Emergency stop mechanism for the entire protocol.
     * Prevents new campaigns and claims in case of a zero-day exploit.
     */
    function pause() external onlyOwner {
        _pause();
    }

    function unpause() external onlyOwner {
        _unpause();
    }

    struct Campaign {
        address advertiser;
        IERC20 token;
        uint256 budget;
        uint256 payout;
        bytes32 verificationGraphHash;
        address oracle;
        address previousOracle;
        uint256 oracleUpdatedAt;
        uint256 createdAt;
    }

    uint256 public constant CANCEL_COOLDOWN = 7 days;
    uint256 public constant ORACLE_GRACE_PERIOD = 1 hours;

    mapping(bytes32 => Campaign) public campaigns;
    mapping(bytes32 => mapping(address => bool)) public hasClaimed;

    event CampaignCreated(bytes32 indexed campaignId, address indexed advertiser, uint256 budget, uint256 payout);
    event PayoutClaimed(bytes32 indexed campaignId, address indexed agent, uint256 amount);
    event CampaignCancelled(bytes32 indexed campaignId, address indexed advertiser, uint256 refund);
    event CampaignExhausted(bytes32 indexed campaignId);
    event OracleUpdated(bytes32 indexed campaignId, address indexed oldOracle, address indexed newOracle);

    function createCampaign(
        bytes32 campaignId,
        IERC20 token,
        uint256 budget,
        uint256 payout,
        bytes32 verificationGraphHash,
        address oracle
    ) external whenNotPaused {
        require(campaigns[campaignId].advertiser == address(0), "Campaign already exists");
        require(payout > 0, "Payout must be positive");
        require(budget >= payout, "Budget must cover at least one payout");
        require(oracle != address(0), "Oracle cannot be zero address");

        uint256 balanceBefore = token.balanceOf(address(this));
        token.safeTransferFrom(msg.sender, address(this), budget);
        uint256 actualBudget = token.balanceOf(address(this)) - balanceBefore;
        require(actualBudget >= payout, "Received budget must cover at least one payout");

        campaigns[campaignId] = Campaign({
            advertiser: msg.sender,
            token: token,
            budget: actualBudget,
            payout: payout,
            verificationGraphHash: verificationGraphHash,
            oracle: oracle,
            previousOracle: address(0),
            oracleUpdatedAt: 0,
            createdAt: block.timestamp
        });

        emit CampaignCreated(campaignId, msg.sender, actualBudget, payout);
    }

    function claimPayout(
        bytes32 campaignId,
        uint256 deadline,
        bytes memory oracleSignature
    ) external nonReentrant whenNotPaused {
        Campaign storage c = campaigns[campaignId];
        require(c.advertiser != address(0), "Campaign does not exist");
        require(block.timestamp <= deadline, "Signature expired");
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

        bool validSigner = (signer == c.oracle);
        if (
            !validSigner &&
            c.previousOracle != address(0) &&
            block.timestamp <= c.oracleUpdatedAt + ORACLE_GRACE_PERIOD
        ) {
            validSigner = (signer == c.previousOracle);
        }
        require(validSigner, "Invalid Oracle Signature");

        hasClaimed[campaignId][msg.sender] = true;
        c.budget -= c.payout;

        c.token.safeTransfer(msg.sender, c.payout);

        emit PayoutClaimed(campaignId, msg.sender, c.payout);

        if (c.budget < c.payout) {
            emit CampaignExhausted(campaignId);
        }
    }

    function updateOracle(bytes32 campaignId, address newOracle) external {
        Campaign storage c = campaigns[campaignId];
        require(c.advertiser == msg.sender, "Only advertiser can update oracle");
        require(newOracle != address(0), "Oracle cannot be zero address");
        require(newOracle != c.oracle, "New oracle must differ from current");

        c.previousOracle = c.oracle;
        c.oracleUpdatedAt = block.timestamp;
        c.oracle = newOracle;

        emit OracleUpdated(campaignId, c.previousOracle, newOracle);
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
