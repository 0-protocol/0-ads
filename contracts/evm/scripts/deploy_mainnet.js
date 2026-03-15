const hre = require("hardhat");

async function main() {
    console.log("🚀 [Sun Force - Mainnet] Initiating AdEscrow Deployment to Base Mainnet");
    console.log("----------------------------------------------------------------------");
    
    const [deployer] = await hre.ethers.getSigners();
    console.log(`[+] Deploying from wallet: ${deployer.address}`);
    
    // Balance check
    const balance = await hre.ethers.provider.getBalance(deployer.address);
    console.log(`[+] Wallet Balance: ${hre.ethers.formatEther(balance)} ETH`);
    
    if (balance === 0n) {
        console.error("❌ ERROR: Insufficient ETH balance on Base Mainnet. Ensure wallet is funded.");
        console.error("⚠️ Sun Force Aborting deployment for safety.");
        process.exit(1);
    }

    const AdEscrow = await hre.ethers.getContractFactory("AdEscrow");
    
    console.log("[~] Estimating deployment gas...");
    const deployTx = await AdEscrow.getDeployTransaction();
    const estimatedGas = await hre.ethers.provider.estimateGas(deployTx);
    console.log(`[+] Estimated Gas: ${estimatedGas.toString()}`);
    
    console.log("[~] Deploying contract...");
    const escrow = await AdEscrow.deploy();
    
    console.log(`[~] Transaction Hash: ${escrow.deploymentTransaction().hash}`);
    console.log("[~] Waiting for block confirmations...");
    await escrow.waitForDeployment();
    
    const address = await escrow.getAddress();
    console.log("✅ -------------------------------------------------------------------");
    console.log(`🎉 AdEscrow successfully deployed to Base Mainnet at: ${address}`);
    console.log("----------------------------------------------------------------------");
    
    // Write out the address for future scripts
    const fs = require('fs');
    fs.writeFileSync('mainnet_address.json', JSON.stringify({ AdEscrow: address }, null, 2));
}

main().catch((error) => {
    console.error("❌ DEPLOYMENT FAILED:", error);
    process.exitCode = 1;
});
