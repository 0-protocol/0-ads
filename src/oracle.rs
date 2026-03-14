use zerolang::VMError;
use reqwest::Client;
use k256::ecdsa::{SigningKey, signature::Signer, VerifyingKey};
use k256::ecdsa::{Signature, RecoveryId};
use sha3::{Digest, Keccak256};
use std::time::Duration;
use dashmap::DashMap;
use tracing::{info, warn};
use zeroize::Zeroize;

/// Trustless Verification Oracle.
///
/// SECURITY NOTE (H-06): This oracle operates with a single ECDSA key.
/// Key compromise exposes all campaigns referencing this oracle address.
/// Future: implement multi-oracle threshold signatures or on-chain key rotation.
pub struct AttentionOracle {
    client: Client,
    oracle_private_key: [u8; 32],
    signature_cache: DashMap<(String, String), Vec<u8>>,
}

impl Drop for AttentionOracle {
    fn drop(&mut self) {
        self.oracle_private_key.zeroize();
    }
}

impl AttentionOracle {
    pub fn new(private_key: [u8; 32]) -> Result<Self, String> {
        if std::env::var("GH_TOKEN").is_err() {
            warn!(
                "GH_TOKEN not set — GitHub API is limited to 60 req/hr. \
                 Set GH_TOKEN for production use (5000 req/hr)."
            );
        }

        let client = Client::builder()
            .user_agent("0-ads-billboard-node/1.0")
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        Ok(Self {
            client,
            oracle_private_key: private_key,
            signature_cache: DashMap::new(),
        })
    }

    pub fn public_address_hex(&self) -> String {
        let signing_key = SigningKey::from_slice(&self.oracle_private_key)
            .expect("Oracle key must be valid at this point");
        let verifying_key = VerifyingKey::from(&signing_key);
        let pubkey_bytes = verifying_key.to_encoded_point(false);
        let pubkey_uncompressed = &pubkey_bytes.as_bytes()[1..]; // skip 0x04 prefix
        let mut hasher = Keccak256::new();
        hasher.update(pubkey_uncompressed);
        let hash = hasher.finalize();
        hex::encode(&hash[12..])
    }

    pub async fn verify_github_star(
        &self,
        agent_github_id: &str,
        target_repo: &str,
        chain_id: u64,
        contract_addr: [u8; 20],
        campaign_id: [u8; 32],
        agent_eth_addr: [u8; 20],
        payout: u64,
        deadline: u64,
    ) -> Result<Vec<u8>, VMError> {
        let cache_key = (hex::encode(campaign_id), hex::encode(agent_eth_addr));
        if let Some(cached) = self.signature_cache.get(&cache_key) {
            info!("Returning cached signature for campaign/agent pair");
            return Ok(cached.value().clone());
        }

        let url = format!(
            "https://api.github.com/users/{}/starred/{}",
            agent_github_id, target_repo
        );

        let mut req = self.client.get(&url);
        if let Ok(token) = std::env::var("GH_TOKEN") {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let resp = req.send().await.map_err(|e| VMError::ExternalResolutionFailed {
            uri: url.clone(),
            reason: format!("Oracle fetch network error: {}", e),
        })?;

        if resp.status().is_success() {
            let signature = self
                .sign_payout(chain_id, contract_addr, campaign_id, agent_eth_addr, payout, deadline)
                .map_err(|e| VMError::ExternalResolutionFailed {
                    uri: url.clone(),
                    reason: e,
                })?;
            self.signature_cache
                .insert(cache_key, signature.clone());
            Ok(signature)
        } else {
            Err(VMError::ExternalResolutionFailed {
                uri: url.clone(),
                reason: "Agent did not star target repo".into(),
            })
        }
    }

    /// Constructs an ABI-encoded payload matching the Solidity contract and signs it.
    /// Payload: abi.encode(chainid, address(this), campaignId, msg.sender, payout, deadline)
    fn sign_payout(
        &self,
        chain_id: u64,
        contract_addr: [u8; 20],
        campaign_id: [u8; 32],
        agent_eth_addr: [u8; 20],
        payout: u64,
        deadline: u64,
    ) -> Result<Vec<u8>, String> {
        let signing_key = SigningKey::from_slice(&self.oracle_private_key)
            .map_err(|_| "Oracle initialized with invalid private key format".to_string())?;

        let mut encoded = Vec::with_capacity(32 * 6);

        // 1. chainid (uint256)
        let mut chain_id_bytes = [0u8; 32];
        chain_id_bytes[24..32].copy_from_slice(&chain_id.to_be_bytes());
        encoded.extend_from_slice(&chain_id_bytes);

        // 2. contract address (address, right-aligned)
        let mut contract_bytes = [0u8; 32];
        contract_bytes[12..32].copy_from_slice(&contract_addr);
        encoded.extend_from_slice(&contract_bytes);

        // 3. campaign_id (bytes32)
        encoded.extend_from_slice(&campaign_id);

        // 4. agent address (address, right-aligned)
        let mut agent_bytes = [0u8; 32];
        agent_bytes[12..32].copy_from_slice(&agent_eth_addr);
        encoded.extend_from_slice(&agent_bytes);

        // 5. payout (uint256)
        let mut payout_bytes = [0u8; 32];
        payout_bytes[24..32].copy_from_slice(&payout.to_be_bytes());
        encoded.extend_from_slice(&payout_bytes);

        // 6. deadline (uint256)
        let mut deadline_bytes = [0u8; 32];
        deadline_bytes[24..32].copy_from_slice(&deadline.to_be_bytes());
        encoded.extend_from_slice(&deadline_bytes);

        let mut hasher = Keccak256::new();
        hasher.update(&encoded);
        let payload_hash = hasher.finalize();

        let prefix = b"\x19Ethereum Signed Message:\n32";
        let mut eth_hasher = Keccak256::new();
        eth_hasher.update(prefix);
        eth_hasher.update(&payload_hash);
        let eth_hash = eth_hasher.finalize();

        let (signature, recovery_id) = signing_key
            .sign_prehash_recoverable(eth_hash.as_slice())
            .map_err(|e| format!("ECDSA Signing failed: {}", e))?;

        let mut sig_bytes = signature.to_bytes().to_vec();
        sig_bytes.push(recovery_id.to_byte() + 27);

        Ok(sig_bytes)
    }
}
