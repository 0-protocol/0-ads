const { expect } = require("chai");
const { ethers } = require("hardhat");

describe("AdEscrow", function () {
  let escrow, token;
  let advertiser, agent, oracle, attacker;

  const CAMPAIGN_ID = ethers.encodeBytes32String("campaign-001");
  const GRAPH_HASH = ethers.encodeBytes32String("graph-hash-001");
  const BUDGET = ethers.parseUnits("100", 18);
  const PAYOUT = ethers.parseUnits("10", 18);

  async function signPayout(oracleSigner, chainId, contractAddr, campaignId, agentAddr, payout, deadline) {
    const encoded = ethers.AbiCoder.defaultAbiCoder().encode(
      ["uint256", "address", "bytes32", "address", "uint256", "uint256"],
      [chainId, contractAddr, campaignId, agentAddr, payout, deadline]
    );
    const payloadHash = ethers.keccak256(encoded);
    return oracleSigner.signMessage(ethers.getBytes(payloadHash));
  }

  beforeEach(async function () {
    [advertiser, agent, oracle, attacker] = await ethers.getSigners();

    const MockToken = await ethers.getContractFactory("MockERC20");
    token = await MockToken.deploy("Mock USDC", "mUSDC", ethers.parseUnits("1000000", 18));
    await token.waitForDeployment();

    const AdEscrow = await ethers.getContractFactory("AdEscrow");
    escrow = await AdEscrow.deploy();
    await escrow.waitForDeployment();

    await token.approve(await escrow.getAddress(), ethers.MaxUint256);
  });

  describe("createCampaign", function () {
    it("should create a campaign and transfer tokens", async function () {
      await escrow.createCampaign(
        CAMPAIGN_ID, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );

      const campaign = await escrow.campaigns(CAMPAIGN_ID);
      expect(campaign.advertiser).to.equal(advertiser.address);
      expect(campaign.budget).to.equal(BUDGET);
      expect(campaign.payout).to.equal(PAYOUT);
      expect(campaign.oracle).to.equal(oracle.address);
    });

    it("should revert on duplicate campaign ID (C-05)", async function () {
      await escrow.createCampaign(
        CAMPAIGN_ID, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );
      await expect(
        escrow.createCampaign(
          CAMPAIGN_ID, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
        )
      ).to.be.revertedWith("Campaign already exists");
    });

    it("should revert if payout is zero (M-02)", async function () {
      await expect(
        escrow.createCampaign(
          CAMPAIGN_ID, await token.getAddress(), BUDGET, 0, GRAPH_HASH, oracle.address
        )
      ).to.be.revertedWith("Payout must be positive");
    });

    it("should revert if budget < payout (M-02)", async function () {
      await expect(
        escrow.createCampaign(
          CAMPAIGN_ID, await token.getAddress(), PAYOUT - 1n, PAYOUT, GRAPH_HASH, oracle.address
        )
      ).to.be.revertedWith("Budget must cover at least one payout");
    });

    it("should emit CampaignCreated with budget", async function () {
      await expect(
        escrow.createCampaign(
          CAMPAIGN_ID, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
        )
      ).to.emit(escrow, "CampaignCreated")
        .withArgs(CAMPAIGN_ID, advertiser.address, BUDGET, PAYOUT);
    });
  });

  describe("claimPayout", function () {
    let deadline;

    beforeEach(async function () {
      await escrow.createCampaign(
        CAMPAIGN_ID, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );
      const block = await ethers.provider.getBlock("latest");
      deadline = block.timestamp + 3600;
    });

    it("should pay out with valid oracle signature", async function () {
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), CAMPAIGN_ID, agent.address, PAYOUT, deadline
      );

      const balBefore = await token.balanceOf(agent.address);
      await escrow.connect(agent).claimPayout(CAMPAIGN_ID, deadline, sig);
      const balAfter = await token.balanceOf(agent.address);

      expect(balAfter - balBefore).to.equal(PAYOUT);
    });

    it("should revert on double claim", async function () {
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), CAMPAIGN_ID, agent.address, PAYOUT, deadline
      );

      await escrow.connect(agent).claimPayout(CAMPAIGN_ID, deadline, sig);
      await expect(
        escrow.connect(agent).claimPayout(CAMPAIGN_ID, deadline, sig)
      ).to.be.revertedWith("Agent already claimed");
    });

    it("should revert with invalid oracle signature", async function () {
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        attacker, chainId, await escrow.getAddress(), CAMPAIGN_ID, agent.address, PAYOUT, deadline
      );

      await expect(
        escrow.connect(agent).claimPayout(CAMPAIGN_ID, deadline, sig)
      ).to.be.revertedWith("Invalid Oracle Signature");
    });

    it("should revert if deadline has passed (M-01)", async function () {
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const expiredDeadline = 1;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), CAMPAIGN_ID, agent.address, PAYOUT, expiredDeadline
      );

      await expect(
        escrow.connect(agent).claimPayout(CAMPAIGN_ID, expiredDeadline, sig)
      ).to.be.revertedWith("Signature expired");
    });

    it("should emit CampaignExhausted when budget drops below payout (L-01)", async function () {
      const smallBudget = PAYOUT;
      const campaignId2 = ethers.encodeBytes32String("campaign-exhaust");
      await escrow.createCampaign(
        campaignId2, await token.getAddress(), smallBudget, PAYOUT, GRAPH_HASH, oracle.address
      );

      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId2, agent.address, PAYOUT, deadline
      );

      await expect(
        escrow.connect(agent).claimPayout(campaignId2, deadline, sig)
      ).to.emit(escrow, "CampaignExhausted")
        .withArgs(campaignId2);
    });

    it("should revert when campaign is empty", async function () {
      const smallBudget = PAYOUT;
      const campaignId3 = ethers.encodeBytes32String("campaign-empty");
      await escrow.createCampaign(
        campaignId3, await token.getAddress(), smallBudget, PAYOUT, GRAPH_HASH, oracle.address
      );

      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig1 = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId3, agent.address, PAYOUT, deadline
      );
      await escrow.connect(agent).claimPayout(campaignId3, deadline, sig1);

      const sig2 = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId3, attacker.address, PAYOUT, deadline
      );
      await expect(
        escrow.connect(attacker).claimPayout(campaignId3, deadline, sig2)
      ).to.be.revertedWith("Campaign empty");
    });
  });

  describe("cancelCampaign", function () {
    beforeEach(async function () {
      await escrow.createCampaign(
        CAMPAIGN_ID, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );
    });

    it("should revert if not advertiser", async function () {
      await expect(
        escrow.connect(attacker).cancelCampaign(CAMPAIGN_ID)
      ).to.be.revertedWith("Only advertiser can cancel");
    });

    it("should revert before cooldown", async function () {
      await expect(
        escrow.cancelCampaign(CAMPAIGN_ID)
      ).to.be.revertedWith("Cancel cooldown not elapsed");
    });

    it("should refund after cooldown (H-01, H-02)", async function () {
      await ethers.provider.send("evm_increaseTime", [7 * 24 * 60 * 60 + 1]);
      await ethers.provider.send("evm_mine");

      const balBefore = await token.balanceOf(advertiser.address);
      await escrow.cancelCampaign(CAMPAIGN_ID);
      const balAfter = await token.balanceOf(advertiser.address);

      expect(balAfter - balBefore).to.equal(BUDGET);
    });

    it("should emit CampaignCancelled", async function () {
      await ethers.provider.send("evm_increaseTime", [7 * 24 * 60 * 60 + 1]);
      await ethers.provider.send("evm_mine");

      await expect(escrow.cancelCampaign(CAMPAIGN_ID))
        .to.emit(escrow, "CampaignCancelled")
        .withArgs(CAMPAIGN_ID, advertiser.address, BUDGET);
    });

    it("should revert if already cancelled (no funds)", async function () {
      await ethers.provider.send("evm_increaseTime", [7 * 24 * 60 * 60 + 1]);
      await ethers.provider.send("evm_mine");

      await escrow.cancelCampaign(CAMPAIGN_ID);
      await expect(
        escrow.cancelCampaign(CAMPAIGN_ID)
      ).to.be.revertedWith("No funds to withdraw");
    });
  });
});
