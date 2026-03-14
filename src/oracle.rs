use zerolang::VMError;
use reqwest::Client;
use k256::ecdsa::{SigningKey, signature::Signer};
use k256::ecdsa::{Signature, RecoveryId};
use sha3::{Digest, Keccak256};
use std::time::Duration;

/// Phase 2: Trustless Verification Oracle
/// This oracle queries Web2 APIs (Twitter/GitHub/Moltbook) to cryptographically sign proof
/// that an Agent actually completed the requested action, allowing the AdEscrow to release funds.

pub struct AttentionOracle {
    client: Client,
    oracle_private_key: [u8; 32], // The Oracle's signing key
}

impl AttentionOracle {
    pub fn new(private_key: [u8; 32]) -> Result<Self, String> {
        let client = Client::builder()
            .user_agent("0-ads-billboard-node/1.0")
            .timeout(Duration::from_secs(10)) // Epic 1: Prevent thread starvation from hanging APIs
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        Ok(Self {
            client,
            oracle_private_key: private_key,
        })
    }

    /// Epic 1: Cryptographic Replay Protection
    /// Verify the action and sign an Ethereum-compatible payload bounded to the smart contract domain.
    pub async fn verify_github_star(
        &self, 
        agent_github_id: &str, 
        target_repo: &str, 
        chain_id: u64, 
        contract_addr: [u8; 20], 
        campaign_id: [u8; 32], 
        agent_eth_addr: [u8; 20], 
        payout: u64
    ) -> Result<Vec<u8>, VMError> {
        let url = format!("https://api.github.com/users/{}/starred/{}", agent_github_id, target_repo);
        
        let mut req = self.client.get(&url);
        if let Ok(token) = std::env::var("GH_TOKEN") {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        
        let resp = req.send().await.map_err(|e| VMError::ExternalResolutionFailed { uri: url.clone(), reason: format!("Oracle fetch network error: {}", e) })?;
        
        if resp.status().is_success() {
            let signature = self.sign_payout(chain_id, contract_addr, campaign_id, agent_eth_addr, payout)
                .map_err(|e| VMError::ExternalResolutionFailed { uri: url.clone(), reason: e })?;
            Ok(signature)
        } else {
            Err(VMError::ExternalResolutionFailed { uri: url.clone(), reason: "Agent did not star target repo".into() })
        }
    }

    /// Constructs an `abi.encode` payload mimicking Solidity to securely sign settlement data.
    fn sign_payout(
        &self, 
        chain_id: u64, 
        contract_addr: [u8; 20], 
        campaign_id: [u8; 32], 
        agent_eth_addr: [u8; 20], 
        payout: u64
    ) -> Result<Vec<u8>, String> {
        let signing_key = SigningKey::from_slice(&self.oracle_private_key)
            .map_err(|_| "Oracle initialized with invalid private key format".to_string())?;
        
        // 32-byte alignment packing for Solidity `abi.encode(chainid, address, bytes32, address, uint256)`
        let mut encoded = Vec::with_capacity(32 * 5);
        
        // 1. chainid (uint256 -> 32 bytes)
        let mut chain_id_bytes = [0u8; 32];
        chain_id_bytes[24..32].copy_from_slice(&chain_id.to_be_bytes());
        encoded.extend_from_slice(&chain_id_bytes);
        
        // 2. contract_addr (address -> 32 bytes, right-aligned)
        let mut contract_bytes = [0u8; 32];
        contract_bytes[12..32].copy_from_slice(&contract_addr);
        encoded.extend_from_slice(&contract_bytes);
        
        // 3. campaign_id (bytes32 -> 32 bytes)
        encoded.extend_from_slice(&campaign_id);
        
        // 4. agent_eth_addr (address -> 32 bytes, right-aligned)
        let mut agent_bytes = [0u8; 32];
        agent_bytes[12..32].copy_from_slice(&agent_eth_addr);
        encoded.extend_from_slice(&agent_bytes);
        
        // 5. payout (uint256 -> 32 bytes)
        let mut payout_bytes = [0u8; 32];
        payout_bytes[24..32].copy_from_slice(&payout.to_be_bytes());
        encoded.extend_from_slice(&payout_bytes);

        // First hash: keccak256 of the ABI encoded struct
        let mut hasher = Keccak256::new();
        hasher.update(&encoded);
        let payload_hash = hasher.finalize();

        // Second hash: \x19Ethereum Signed Message:\n32 + payload_hash
        let prefix = b"\x19Ethereum Signed Message:\n32";
        let mut eth_hasher = Keccak256::new();
        eth_hasher.update(prefix);
        eth_hasher.update(&payload_hash);
        let eth_hash = eth_hasher.finalize();

        // Sign the pre-hashed message
        let (signature, recovery_id) = signing_key.sign_prehash_recoverable(eth_hash.as_slice())
            .map_err(|e| format!("ECDSA Signing failed: {}", e))?;
        
        let mut sig_bytes = signature.to_bytes().to_vec();
        // Ethereum recovery ID standard (+27)
        sig_bytes.push(recovery_id.to_byte() + 27); 

        Ok(sig_bytes)
    }
}
