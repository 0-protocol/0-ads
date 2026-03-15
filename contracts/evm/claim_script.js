const hre = require("hardhat");
const axios = require("axios");

async function main() {
  const agentPrivateKey = process.env.PRIVATE_KEY;
  const githubId = "sjhddh";
  const repo = "0-protocol/0-lang";
  const campaignId = "0x0000000000000000000000000000000000000000000000000000000000000001";
  const payoutAmount = 1000000; // 1 USDC
  const chainId = 84532;
  const contractAddr = "0x8a2aD6bC4A240515c49035bE280BacB7CA94afC4";

  const wallet = new hre.ethers.Wallet(agentPrivateKey, hre.ethers.provider);
  console.log(`🤖 Agent Identity: ${wallet.address}`);

  console.log("\n[2] Generating Wallet Ownership Proof...");
  const bindTimestamp = Math.floor(Date.now() / 1000);
  const msg = `0-ads-wallet-bind:${githubId}:${bindTimestamp}`;
  const walletSig = await wallet.signMessage(msg);

  const payload = {
      agent_github_id: githubId,
      target_repo: repo,
      chain_id: chainId,
      contract_addr: contractAddr,
      campaign_id: campaignId,
      agent_eth_addr: wallet.address,
      payout: payoutAmount,
      deadline: Math.floor(Date.now() / 1000) + 3600,
      wallet_sig: walletSig,
      bind_timestamp: bindTimestamp
  };

  console.log("\n[3] Requesting Cryptographic Proof from 0-ads Oracle...");
  const ORACLE_URL = "https://ads.0-protocol.org/api/v1/oracle/verify";
  
  let resData;
  try {
    const response = await axios.post(ORACLE_URL, payload);
    resData = response.data;
  } catch (error) {
    if (error.response) {
      console.log(`❌ Oracle Error: ${JSON.stringify(error.response.data)}`);
    } else {
      console.log(`❌ Failed to reach Oracle: ${error.message}`);
    }
    return;
  }

  if (resData.error) {
      console.log(`❌ Oracle rejected claim: ${resData.error}`);
      return;
  }

  let signature = resData.signature;
  if (!signature.startsWith("0x")) signature = "0x" + signature;
      
  console.log(`✅ Oracle verified action and signed proof!`);
  console.log(`🔑 Signature: ${signature.substring(0, 14)}...${signature.substring(signature.length-10)}`);

  console.log("\n[4] Submitting Proof to Base Sepolia L2 AdEscrow Contract...");
  const AdEscrow = await hre.ethers.getContractAt("AdEscrow", contractAddr, wallet);
  
  try {
      const tx = await AdEscrow.claimPayout(
          campaignId,
          payload.deadline,
          signature
      );
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
