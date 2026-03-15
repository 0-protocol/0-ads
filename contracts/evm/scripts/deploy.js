const hre = require("hardhat");

async function main() {
  const AdEscrow = await hre.ethers.getContractFactory("AdEscrow");
  const escrow = await AdEscrow.deploy();
  await escrow.waitForDeployment();

  const escrowAddr = await escrow.getAddress();
  const deployer = (await hre.ethers.getSigners())[0];
  console.log(`AdEscrow deployed to: ${escrowAddr}`);
  console.log(`Current owner (deployer): ${deployer.address}`);

  // Post-deploy ownership transfer to multisig/Safe.
  // Set SAFE_ADDRESS env var to transfer ownership immediately after deploy.
  const safeAddress = process.env.SAFE_ADDRESS;
  if (safeAddress) {
    console.log(`Transferring ownership to Safe: ${safeAddress}`);
    const tx = await escrow.transferOwnership(safeAddress);
    await tx.wait();
    console.log(`Ownership transferred. New owner: ${await escrow.owner()}`);
    console.log("IMPORTANT: pause() and unpause() now require the Safe to sign.");
  } else {
    console.log(
      "WARNING: SAFE_ADDRESS not set. Owner remains the deployer EOA.\n" +
      "For production, transfer ownership to a multisig:\n" +
      "  SAFE_ADDRESS=0x... npx hardhat run scripts/deploy.js --network base"
    );
  }

  // Optional: seed a campaign right after deploy using the current
  // createCampaign(token, budget, payout, graphHash, oracle) signature.
  const seedToken = process.env.SEED_TOKEN;
  const seedBudget = process.env.SEED_BUDGET;
  const seedPayout = process.env.SEED_PAYOUT;
  const seedGraphHash = process.env.SEED_GRAPH_HASH;
  const seedOracle = process.env.SEED_ORACLE;
  const shouldSeed =
    seedToken && seedBudget && seedPayout && seedGraphHash && seedOracle;

  if (shouldSeed) {
    console.log("Seeding initial campaign with createCampaign(...)");
    const tx = await escrow.createCampaign(
      seedToken,
      seedBudget,
      seedPayout,
      seedGraphHash,
      seedOracle
    );
    const receipt = await tx.wait();
    const created = receipt.logs
      .map((log) => {
        try {
          return escrow.interface.parseLog(log);
        } catch (_) {
          return null;
        }
      })
      .find((event) => event && event.name === "CampaignCreated");

    if (!created) {
      throw new Error(
        "CampaignCreated event not found. Could not extract campaign ID."
      );
    }
    console.log(`Seed campaign created with ID: ${created.args.campaignId}`);
  } else {
    console.log(
      "Skipping seed campaign. To create one now, set all env vars:\n" +
        "  SEED_TOKEN, SEED_BUDGET, SEED_PAYOUT, SEED_GRAPH_HASH, SEED_ORACLE"
    );
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
