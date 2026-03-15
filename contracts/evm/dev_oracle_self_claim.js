/**
 * DEVELOPMENT-ONLY SCRIPT.
 *
 * This script demonstrates a self-claim flow where the signer also acts as the
 * configured campaign oracle. It is NOT a bypass tool and does not circumvent
 * contract verification. It only succeeds when PRIVATE_KEY controls the oracle
 * address configured for the target campaign.
 */
const hre = require("hardhat");

async function main() {
  const privateKey = process.env.PRIVATE_KEY;
  if (!privateKey) {
    throw new Error("PRIVATE_KEY is required");
  }

  const campaignId = "0x0000000000000000000000000000000000000000000000000000000000000001";
  const payoutAmount = 1000000; // 1 USDC (6 decimals)
  const chainId = 84532;
  const contractAddr = "0x8a2aD6bC4A240515c49035bE280BacB7CA94afC4";

  const wallet = new hre.ethers.Wallet(privateKey, hre.ethers.provider);
  console.log(`🤖 Agent/Oracle Identity: ${wallet.address}`);
  console.log("⚠️ Dev-only: requires signer to be configured campaign oracle.");

  const deadline = Math.floor(Date.now() / 1000) + 3600;

  const abiCoder = new hre.ethers.AbiCoder();
  const encodedData = abiCoder.encode(
    ["uint256", "address", "bytes32", "address", "uint256", "uint256"],
    [chainId, contractAddr, campaignId, wallet.address, payoutAmount, deadline]
  );

  const payloadHash = hre.ethers.keccak256(encodedData);
  const signature = await wallet.signMessage(hre.ethers.getBytes(payloadHash));
  console.log("✅ Oracle-compatible signature generated");

  const AdEscrow = await hre.ethers.getContractAt("AdEscrow", contractAddr, wallet);

  try {
    const tx = await AdEscrow.claimPayout(campaignId, deadline, signature);
    console.log("Transaction sent! Waiting for confirmation...");
    const receipt = await tx.wait();
    console.log(`🎉 Success! Payout transaction confirmed in block ${receipt.blockNumber}`);
    console.log(`🔗 View on Basescan: https://sepolia.basescan.org/tx/${receipt.hash}`);
  } catch (e) {
    console.log(`❌ Blockchain transaction failed: ${e.message}`);
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
