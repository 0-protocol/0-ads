const hre = require("hardhat");

async function main() {
  const USD0 = await hre.ethers.getContractFactory("USD0");
  const usd0 = await USD0.deploy();
  await usd0.waitForDeployment();
  console.log(`USD0 deployed to: ${await usd0.getAddress()}`);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
