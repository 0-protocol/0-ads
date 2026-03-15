const hre = require("hardhat");

async function main() {
  const DevnetUSDC = await hre.ethers.getContractFactory("DevnetUSDC");
  const dusdc = await DevnetUSDC.deploy();
  await dusdc.waitForDeployment();
  console.log(`Devnet USDC deployed to: ${await dusdc.getAddress()}`);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
