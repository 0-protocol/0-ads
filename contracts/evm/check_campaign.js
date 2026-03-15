const hre = require("hardhat");

async function main() {
  const privateKey = process.env.PRIVATE_KEY;
  const campaignId = "0x0000000000000000000000000000000000000000000000000000000000000000";
  const contractAddr = "0x8a2aD6bC4A240515c49035bE280BacB7CA94afC4";

  const wallet = new hre.ethers.Wallet(privateKey, hre.ethers.provider);
  const AdEscrow = await hre.ethers.getContractAt("AdEscrow", contractAddr, wallet);
  
  const c = await AdEscrow.campaigns(campaignId);
  console.log("Advertiser:", c.advertiser);
  console.log("Token:", c.token);
  console.log("Budget:", c.budget.toString());
  console.log("Payout:", c.payout.toString());
  console.log("Oracle:", c.oracle);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
