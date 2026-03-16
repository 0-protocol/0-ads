# 🚨 V1 CONTRACT PERMANENTLY DEPRECATED AND COMPROMISED 🚨

## Incident Report
The V1 legacy smart contract for `0-ads` is officially **DEAD**. 
During a hard fork operation, funds were swept to the original deployer wallet (`0x7E89024404039FF9C0A65c8bbc91A6503d954dFf`), whose private key had been previously compromised in a GitHub leak. MEV sweeper bots immediately drained the 499 USDC.

## Actions Taken
- All REST API endpoints pointing to the V1 contract have been permanently shut down.
- The V1 contract address is blacklisted in our front-ends and SDKs.
- **DO NOT INTERACT WITH THE V1 CONTRACT.** Any funds sent to it or its deployer will be instantly lost to MEV bots.

## V2 ZK-Native Deployment (Phase 32)
The new `0-ads` ecosystem will operate entirely on the Phase 32 ZK-Native infrastructure. 
A completely new, secure cold wallet is being generated for the V2 deployment. No private keys will ever touch this repository again.
