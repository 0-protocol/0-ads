const hre = require("hardhat");

async function main() {
  const AdEscrow = await hre.ethers.getContractFactory("AdEscrow");
  const escrow = await AdEscrow.deploy();
  await escrow.waitForDeployment();
  console.log(`AdEscrow deployed to: ${await escrow.getAddress()}`);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
