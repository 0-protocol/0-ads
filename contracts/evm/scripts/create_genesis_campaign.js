const hre = require("hardhat");
const fs = require("fs");

async function main() {
    console.log("🚀 [Sun Force - Mainnet] Initiating Genesis Campaign Creation");
    console.log("----------------------------------------------------------------------");

    const [deployer] = await hre.ethers.getSigners();
    console.log(`[+] Managing from wallet: ${deployer.address}`);
    
    // Read the contract address
    if (!fs.existsSync('mainnet_address.json')) {
        console.error("❌ ERROR: AdEscrow address not found. Did you run the deployment script?");
        process.exit(1);
    }
    const escrowAddr = JSON.parse(fs.readFileSync('mainnet_address.json')).AdEscrow;
    console.log(`[+] AdEscrow Address: ${escrowAddr}`);

    const AdEscrow = await hre.ethers.getContractAt("AdEscrow", escrowAddr);
    
    // Official Base Mainnet USDC Contract
    const usdcAddr = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
    const IERC20 = await hre.ethers.getContractAt("IERC20", usdcAddr);
    
    // 1. Check USDC Balance
    const balance = await IERC20.balanceOf(deployer.address);
    console.log(`[+] Wallet USDC Balance: ${hre.ethers.formatUnits(balance, 6)} USDC`);
    
    const TOTAL_BUDGET = hre.ethers.parseUnits("500", 6); // 500 USDC
    const PAYOUT = hre.ethers.parseUnits("1", 6);         // 1 USDC per action
    
    if (balance < TOTAL_BUDGET) {
        console.error(`❌ ERROR: Need 500 USDC to seed the Genesis Campaign. Only have ${hre.ethers.formatUnits(balance, 6)}.`);
        process.exit(1);
    }

    // 2. Approve AdEscrow to spend USDC
    console.log(`[~] Approving AdEscrow to spend 500 USDC...`);
    let tx = await IERC20.approve(escrowAddr, TOTAL_BUDGET);
    console.log(`[~] Approve Tx: ${tx.hash}`);
    await tx.wait();
    console.log(`[+] Approval confirmed.`);

    // 3. Create Campaign
    const CAMPAIGN_ID = "0x0000000000000000000000000000000000000000000000000000000000000001";
    // Mock hash of the verification graph JSON defining the task
    const VERIFICATION_GRAPH_HASH = hre.ethers.keccak256(hre.ethers.toUtf8Bytes("github_star:0-protocol/0-lang"));
    // Oracle public key (can be same as deployer for now or a dedicated oracle node key)
    const ORACLE_ADDR = deployer.address;

    console.log(`[~] Creating Campaign ${CAMPAIGN_ID}...`);
    tx = await AdEscrow.createCampaign(
        CAMPAIGN_ID,
        usdcAddr,
        TOTAL_BUDGET,
        PAYOUT,
        VERIFICATION_GRAPH_HASH,
        ORACLE_ADDR
    );
    console.log(`[~] Create Campaign Tx: ${tx.hash}`);
    await tx.wait();
    
    console.log("✅ -------------------------------------------------------------------");
    console.log(`🎉 Campaign Successfully Created and Seeded with 500 USDC!`);
    console.log("----------------------------------------------------------------------");
}

main().catch((error) => {
    console.error("❌ CAMPAIGN CREATION FAILED:", error);
    process.exitCode = 1;
});
