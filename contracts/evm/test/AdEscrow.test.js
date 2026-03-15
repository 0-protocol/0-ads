const { expect } = require("chai");
const { ethers } = require("hardhat");

describe("AdEscrow", function () {
  let escrow, token;
  let advertiser, agent, oracle, attacker;

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

  async function createCampaignAndGetId(escrowContract, tokenAddr, budget, payout, graphHash, oracleAddr) {
    const tx = await escrowContract.createCampaign(tokenAddr, budget, payout, graphHash, oracleAddr);
    const receipt = await tx.wait();
    const event = receipt.logs.find(
      (log) => {
        try { return escrowContract.interface.parseLog(log)?.name === "CampaignCreated"; }
        catch { return false; }
      }
    );
    const parsed = escrowContract.interface.parseLog(event);
    return parsed.args.campaignId;
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
    it("should create a campaign with derived ID and transfer tokens", async function () {
      const campaignId = await createCampaignAndGetId(
        escrow, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );

      const campaign = await escrow.campaigns(campaignId);
      expect(campaign.advertiser).to.equal(advertiser.address);
      expect(campaign.budget).to.equal(BUDGET);
      expect(campaign.payout).to.equal(PAYOUT);
      expect(campaign.oracle).to.equal(oracle.address);
    });

    it("should generate unique IDs for successive campaigns", async function () {
      const id1 = await createCampaignAndGetId(
        escrow, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );
      const id2 = await createCampaignAndGetId(
        escrow, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );
      expect(id1).to.not.equal(id2);
    });

    it("should revert if payout is zero", async function () {
      await expect(
        escrow.createCampaign(await token.getAddress(), BUDGET, 0, GRAPH_HASH, oracle.address)
      ).to.be.revertedWith("Payout must be positive");
    });

    it("should revert if budget < payout", async function () {
      await expect(
        escrow.createCampaign(await token.getAddress(), PAYOUT - 1n, PAYOUT, GRAPH_HASH, oracle.address)
      ).to.be.revertedWith("Budget must cover at least one payout");
    });

    it("should revert if oracle is zero address", async function () {
      await expect(
        escrow.createCampaign(await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, ethers.ZeroAddress)
      ).to.be.revertedWith("Oracle cannot be zero address");
    });

    it("should emit CampaignCreated with derived ID", async function () {
      const expectedId = await escrow.previewCampaignId(advertiser.address);
      await expect(
        escrow.createCampaign(await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address)
      ).to.emit(escrow, "CampaignCreated")
        .withArgs(expectedId, advertiser.address, BUDGET, PAYOUT);
    });

    it("should handle fee-on-transfer tokens correctly", async function () {
      const MockFeeToken = await ethers.getContractFactory("MockFeeToken");
      const feeToken = await MockFeeToken.deploy("Fee Token", "FEE", ethers.parseUnits("1000000", 18));
      await feeToken.waitForDeployment();
      await feeToken.approve(await escrow.getAddress(), ethers.MaxUint256);

      const budget = ethers.parseUnits("1000", 18);
      const payout = ethers.parseUnits("5", 18);

      const campaignId = await createCampaignAndGetId(
        escrow, await feeToken.getAddress(), budget, payout, GRAPH_HASH, oracle.address
      );

      const campaign = await escrow.campaigns(campaignId);
      expect(campaign.budget).to.equal(ethers.parseUnits("990", 18));
    });
  });

  describe("previewCampaignId", function () {
    it("should return the same ID that createCampaign will use", async function () {
      const preview = await escrow.previewCampaignId(advertiser.address);
      const actual = await createCampaignAndGetId(
        escrow, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );
      expect(preview).to.equal(actual);
    });

    it("should return different IDs for different senders", async function () {
      const idA = await escrow.previewCampaignId(advertiser.address);
      const idB = await escrow.previewCampaignId(agent.address);
      expect(idA).to.not.equal(idB);
    });

    it("should advance after a campaign is created", async function () {
      const preview1 = await escrow.previewCampaignId(advertiser.address);
      await createCampaignAndGetId(
        escrow, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );
      const preview2 = await escrow.previewCampaignId(advertiser.address);
      expect(preview1).to.not.equal(preview2);
    });
  });

  describe("claimPayout", function () {
    let campaignId, deadline;

    beforeEach(async function () {
      campaignId = await createCampaignAndGetId(
        escrow, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );
      const block = await ethers.provider.getBlock("latest");
      deadline = block.timestamp + 3600;
    });

    it("should pay out with valid oracle signature", async function () {
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, deadline
      );

      const balBefore = await token.balanceOf(agent.address);
      await escrow.connect(agent).claimPayout(campaignId, deadline, sig);
      const balAfter = await token.balanceOf(agent.address);

      expect(balAfter - balBefore).to.equal(PAYOUT);
    });

    it("should revert on double claim", async function () {
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, deadline
      );

      await escrow.connect(agent).claimPayout(campaignId, deadline, sig);
      await expect(
        escrow.connect(agent).claimPayout(campaignId, deadline, sig)
      ).to.be.revertedWith("Agent already claimed");
    });

    it("should revert with invalid oracle signature", async function () {
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        attacker, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, deadline
      );

      await expect(
        escrow.connect(agent).claimPayout(campaignId, deadline, sig)
      ).to.be.revertedWith("Invalid Oracle Signature");
    });

    it("should revert if deadline has passed", async function () {
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const expiredDeadline = 1;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, expiredDeadline
      );

      await expect(
        escrow.connect(agent).claimPayout(campaignId, expiredDeadline, sig)
      ).to.be.revertedWith("Signature expired");
    });

    it("should revert for non-existent campaign", async function () {
      const fakeCampaignId = ethers.encodeBytes32String("doesnt-exist");
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), fakeCampaignId, agent.address, PAYOUT, deadline
      );

      await expect(
        escrow.connect(agent).claimPayout(fakeCampaignId, deadline, sig)
      ).to.be.revertedWith("Campaign does not exist");
    });

    it("should emit CampaignExhausted when budget drops below payout", async function () {
      const smallBudget = PAYOUT;
      const campaignId2 = await createCampaignAndGetId(
        escrow, await token.getAddress(), smallBudget, PAYOUT, GRAPH_HASH, oracle.address
      );

      const chainId = (await ethers.provider.getNetwork()).chainId;
      const block = await ethers.provider.getBlock("latest");
      const dl = block.timestamp + 3600;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId2, agent.address, PAYOUT, dl
      );

      await expect(
        escrow.connect(agent).claimPayout(campaignId2, dl, sig)
      ).to.emit(escrow, "CampaignExhausted")
        .withArgs(campaignId2);
    });

    it("should revert when campaign is empty", async function () {
      const smallBudget = PAYOUT;
      const campaignId3 = await createCampaignAndGetId(
        escrow, await token.getAddress(), smallBudget, PAYOUT, GRAPH_HASH, oracle.address
      );

      const chainId = (await ethers.provider.getNetwork()).chainId;
      const block = await ethers.provider.getBlock("latest");
      const dl = block.timestamp + 3600;
      const sig1 = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId3, agent.address, PAYOUT, dl
      );
      await escrow.connect(agent).claimPayout(campaignId3, dl, sig1);

      const sig2 = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId3, attacker.address, PAYOUT, dl
      );
      await expect(
        escrow.connect(attacker).claimPayout(campaignId3, dl, sig2)
      ).to.be.revertedWith("Campaign empty");
    });
  });

  describe("MAX_DEADLINE_WINDOW", function () {
    let campaignId;

    beforeEach(async function () {
      campaignId = await createCampaignAndGetId(
        escrow, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );
    });

    it("should revert when deadline is more than 2 hours in the future", async function () {
      const block = await ethers.provider.getBlock("latest");
      const farDeadline = block.timestamp + 3 * 3600;
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, farDeadline
      );

      await expect(
        escrow.connect(agent).claimPayout(campaignId, farDeadline, sig)
      ).to.be.revertedWith("Deadline too far in future");
    });

    it("should succeed when deadline is within 2 hours", async function () {
      const block = await ethers.provider.getBlock("latest");
      const okDeadline = block.timestamp + 3600;
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, okDeadline
      );

      const balBefore = await token.balanceOf(agent.address);
      await escrow.connect(agent).claimPayout(campaignId, okDeadline, sig);
      const balAfter = await token.balanceOf(agent.address);
      expect(balAfter - balBefore).to.equal(PAYOUT);
    });
  });

  describe("cancelCampaign", function () {
    let campaignId;

    beforeEach(async function () {
      campaignId = await createCampaignAndGetId(
        escrow, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );
    });

    it("should revert if not advertiser", async function () {
      await expect(
        escrow.connect(attacker).cancelCampaign(campaignId)
      ).to.be.revertedWith("Only advertiser can cancel");
    });

    it("should revert before cooldown", async function () {
      await expect(
        escrow.cancelCampaign(campaignId)
      ).to.be.revertedWith("Cancel cooldown not elapsed");
    });

    it("should refund after cooldown", async function () {
      await ethers.provider.send("evm_increaseTime", [7 * 24 * 60 * 60 + 1]);
      await ethers.provider.send("evm_mine");

      const balBefore = await token.balanceOf(advertiser.address);
      await escrow.cancelCampaign(campaignId);
      const balAfter = await token.balanceOf(advertiser.address);

      expect(balAfter - balBefore).to.equal(BUDGET);
    });

    it("should emit CampaignCancelled", async function () {
      await ethers.provider.send("evm_increaseTime", [7 * 24 * 60 * 60 + 1]);
      await ethers.provider.send("evm_mine");

      await expect(escrow.cancelCampaign(campaignId))
        .to.emit(escrow, "CampaignCancelled")
        .withArgs(campaignId, advertiser.address, BUDGET);
    });

    it("should revert if already cancelled (no funds)", async function () {
      await ethers.provider.send("evm_increaseTime", [7 * 24 * 60 * 60 + 1]);
      await ethers.provider.send("evm_mine");

      await escrow.cancelCampaign(campaignId);
      await expect(
        escrow.cancelCampaign(campaignId)
      ).to.be.revertedWith("No funds to withdraw");
    });
  });

  describe("sweepDust", function () {
    let campaignId;

    beforeEach(async function () {
      const smallBudget = PAYOUT + ethers.parseUnits("3", 18);
      campaignId = await createCampaignAndGetId(
        escrow, await token.getAddress(), smallBudget, PAYOUT, GRAPH_HASH, oracle.address
      );

      const chainId = (await ethers.provider.getNetwork()).chainId;
      const block = await ethers.provider.getBlock("latest");
      const deadline = block.timestamp + 3600;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, deadline
      );
      await escrow.connect(agent).claimPayout(campaignId, deadline, sig);
    });

    it("should allow advertiser to sweep dust from exhausted campaign", async function () {
      const campaign = await escrow.campaigns(campaignId);
      const dust = campaign.budget;
      expect(dust).to.be.gt(0);
      expect(dust).to.be.lt(PAYOUT);

      const balBefore = await token.balanceOf(advertiser.address);
      await escrow.sweepDust(campaignId);
      const balAfter = await token.balanceOf(advertiser.address);
      expect(balAfter - balBefore).to.equal(dust);
    });

    it("should revert if not advertiser", async function () {
      await expect(
        escrow.connect(attacker).sweepDust(campaignId)
      ).to.be.revertedWith("Only advertiser can sweep");
    });

    it("should revert if campaign is still active (budget >= payout)", async function () {
      const activeCampaignId = await createCampaignAndGetId(
        escrow, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );
      await expect(
        escrow.sweepDust(activeCampaignId)
      ).to.be.revertedWith("Campaign still active");
    });

    it("should revert if no dust (budget is zero)", async function () {
      await escrow.sweepDust(campaignId);
      await expect(
        escrow.sweepDust(campaignId)
      ).to.be.revertedWith("No dust to sweep");
    });

    it("should emit DustSwept", async function () {
      const campaign = await escrow.campaigns(campaignId);
      const dust = campaign.budget;

      await expect(escrow.sweepDust(campaignId))
        .to.emit(escrow, "DustSwept")
        .withArgs(campaignId, advertiser.address, dust);
    });
  });

  describe("updateOracle", function () {
    let campaignId;

    beforeEach(async function () {
      campaignId = await createCampaignAndGetId(
        escrow, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );
    });

    it("should allow advertiser to rotate oracle key", async function () {
      await expect(escrow.updateOracle(campaignId, attacker.address))
        .to.emit(escrow, "OracleUpdated")
        .withArgs(campaignId, oracle.address, attacker.address);

      const campaign = await escrow.campaigns(campaignId);
      expect(campaign.oracle).to.equal(attacker.address);
    });

    it("should reject non-advertiser oracle update", async function () {
      await expect(
        escrow.connect(attacker).updateOracle(campaignId, attacker.address)
      ).to.be.revertedWith("Only advertiser can update oracle");
    });

    it("should reject zero address oracle", async function () {
      await expect(
        escrow.updateOracle(campaignId, ethers.ZeroAddress)
      ).to.be.revertedWith("Oracle cannot be zero address");
    });

    it("should reject same oracle address", async function () {
      await expect(
        escrow.updateOracle(campaignId, oracle.address)
      ).to.be.revertedWith("New oracle must differ from current");
    });

    it("should work with new oracle for claims", async function () {
      await escrow.updateOracle(campaignId, attacker.address);

      const block = await ethers.provider.getBlock("latest");
      const deadline = block.timestamp + 3600;
      const chainId = (await ethers.provider.getNetwork()).chainId;

      const sig = await signPayout(
        attacker, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, deadline
      );

      const balBefore = await token.balanceOf(agent.address);
      await escrow.connect(agent).claimPayout(campaignId, deadline, sig);
      const balAfter = await token.balanceOf(agent.address);

      expect(balAfter - balBefore).to.equal(PAYOUT);
    });

    it("should accept old oracle signature within grace period", async function () {
      const block = await ethers.provider.getBlock("latest");
      const deadline = block.timestamp + 3600;
      const chainId = (await ethers.provider.getNetwork()).chainId;

      const sigFromOldOracle = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, deadline
      );

      await escrow.updateOracle(campaignId, attacker.address);

      const balBefore = await token.balanceOf(agent.address);
      await escrow.connect(agent).claimPayout(campaignId, deadline, sigFromOldOracle);
      const balAfter = await token.balanceOf(agent.address);

      expect(balAfter - balBefore).to.equal(PAYOUT);
    });

    it("should reject old oracle signature after grace period", async function () {
      const block = await ethers.provider.getBlock("latest");
      const deadline = block.timestamp + 7200;
      const chainId = (await ethers.provider.getNetwork()).chainId;

      const sigFromOldOracle = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, deadline
      );

      await escrow.updateOracle(campaignId, attacker.address);

      await ethers.provider.send("evm_increaseTime", [3601]);
      await ethers.provider.send("evm_mine");

      await expect(
        escrow.connect(agent).claimPayout(campaignId, deadline, sigFromOldOracle)
      ).to.be.revertedWith("Invalid Oracle Signature");
    });
  });

  describe("claimPayoutFor (delegated / gasless relayer)", function () {
    let campaignId, deadline, relayer;

    beforeEach(async function () {
      [advertiser, agent, oracle, attacker, relayer] = await ethers.getSigners();

      const MockToken = await ethers.getContractFactory("MockERC20");
      token = await MockToken.deploy("Mock USDC", "mUSDC", ethers.parseUnits("1000000", 18));
      await token.waitForDeployment();

      const AdEscrow = await ethers.getContractFactory("AdEscrow");
      escrow = await AdEscrow.deploy();
      await escrow.waitForDeployment();

      await token.approve(await escrow.getAddress(), ethers.MaxUint256);

      campaignId = await createCampaignAndGetId(
        escrow, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );
      const block = await ethers.provider.getBlock("latest");
      deadline = block.timestamp + 3600;
    });

    it("should pay agent (not relayer) when relayer submits on behalf of agent", async function () {
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, deadline
      );

      const agentBefore = await token.balanceOf(agent.address);
      const relayerBefore = await token.balanceOf(relayer.address);

      await escrow.connect(relayer).claimPayoutFor(campaignId, agent.address, deadline, sig);

      const agentAfter = await token.balanceOf(agent.address);
      const relayerAfter = await token.balanceOf(relayer.address);

      expect(agentAfter - agentBefore).to.equal(PAYOUT);
      expect(relayerAfter - relayerBefore).to.equal(0n);
    });

    it("should revert if relayer tries to redirect funds to itself via mismatched agent", async function () {
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sigForAgent = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, deadline
      );

      await expect(
        escrow.connect(relayer).claimPayoutFor(campaignId, relayer.address, deadline, sigForAgent)
      ).to.be.revertedWith("Invalid Oracle Signature");
    });

    it("should revert on double claim via delegated path", async function () {
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, deadline
      );

      await escrow.connect(relayer).claimPayoutFor(campaignId, agent.address, deadline, sig);
      await expect(
        escrow.connect(relayer).claimPayoutFor(campaignId, agent.address, deadline, sig)
      ).to.be.revertedWith("Agent already claimed");
    });

    it("should prevent double claim across direct and delegated paths", async function () {
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, deadline
      );

      await escrow.connect(agent).claimPayout(campaignId, deadline, sig);
      await expect(
        escrow.connect(relayer).claimPayoutFor(campaignId, agent.address, deadline, sig)
      ).to.be.revertedWith("Agent already claimed");
    });

    it("should revert if agent is zero address", async function () {
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId, ethers.ZeroAddress, PAYOUT, deadline
      );

      await expect(
        escrow.connect(relayer).claimPayoutFor(campaignId, ethers.ZeroAddress, deadline, sig)
      ).to.be.revertedWith("Agent cannot be zero address");
    });

    it("should emit PayoutClaimed with agent address (not relayer)", async function () {
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, deadline
      );

      await expect(
        escrow.connect(relayer).claimPayoutFor(campaignId, agent.address, deadline, sig)
      ).to.emit(escrow, "PayoutClaimed")
        .withArgs(campaignId, agent.address, PAYOUT);
    });
  });

  describe("Pause guards", function () {
    let campaignId;

    beforeEach(async function () {
      campaignId = await createCampaignAndGetId(
        escrow, await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address
      );
      await escrow.pause();
    });

    it("should revert createCampaign when paused", async function () {
      await expect(
        escrow.createCampaign(await token.getAddress(), BUDGET, PAYOUT, GRAPH_HASH, oracle.address)
      ).to.be.revertedWithCustomError(escrow, "EnforcedPause");
    });

    it("should revert claimPayout when paused", async function () {
      const block = await ethers.provider.getBlock("latest");
      const deadline = block.timestamp + 3600;
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), campaignId, agent.address, PAYOUT, deadline
      );

      await expect(
        escrow.connect(agent).claimPayout(campaignId, deadline, sig)
      ).to.be.revertedWithCustomError(escrow, "EnforcedPause");
    });

    it("should revert cancelCampaign when paused", async function () {
      await ethers.provider.send("evm_increaseTime", [7 * 24 * 60 * 60 + 1]);
      await ethers.provider.send("evm_mine");

      await expect(
        escrow.cancelCampaign(campaignId)
      ).to.be.revertedWithCustomError(escrow, "EnforcedPause");
    });

    it("should revert updateOracle when paused", async function () {
      await expect(
        escrow.updateOracle(campaignId, attacker.address)
      ).to.be.revertedWithCustomError(escrow, "EnforcedPause");
    });

    it("should revert sweepDust when paused", async function () {
      await escrow.unpause();

      const smallBudget = PAYOUT + ethers.parseUnits("3", 18);
      const dustCampaignId = await createCampaignAndGetId(
        escrow, await token.getAddress(), smallBudget, PAYOUT, GRAPH_HASH, oracle.address
      );

      const chainId = (await ethers.provider.getNetwork()).chainId;
      const block = await ethers.provider.getBlock("latest");
      const deadline = block.timestamp + 3600;
      const sig = await signPayout(
        oracle, chainId, await escrow.getAddress(), dustCampaignId, agent.address, PAYOUT, deadline
      );
      await escrow.connect(agent).claimPayout(dustCampaignId, deadline, sig);

      await escrow.pause();

      await expect(
        escrow.sweepDust(dustCampaignId)
      ).to.be.revertedWithCustomError(escrow, "EnforcedPause");
    });
  });
});
