use zerolang::VMError;
use reqwest::Client;
use k256::ecdsa::{SigningKey, Signature, signature::Signer};

/// Phase 2: Trustless Verification Oracle
/// This oracle queries Web2 APIs (Twitter/GitHub/Moltbook) to cryptographically sign proof
/// that an Agent actually completed the requested action, allowing the AdEscrow to release funds.

pub struct AttentionOracle {
    client: Client,
    oracle_private_key: [u8; 32], // The Oracle's signing key
}

impl AttentionOracle {
    pub fn new(private_key: [u8; 32]) -> Self {
        Self {
            client: Client::builder()
                .user_agent("0-ads-billboard-node/1.0")
                .build()
                .unwrap(),
            oracle_private_key: private_key,
        }
    }

    pub async fn verify_github_star(&self, agent_github_id: &str, target_repo: &str) -> Result<Vec<u8>, VMError> {
        let url = format!("https://api.github.com/users/{}/starred/{}", agent_github_id, target_repo);
        
        let mut req = self.client.get(&url);
        if let Ok(token) = std::env::var("GH_TOKEN") {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        
        let resp = req.send().await.map_err(|_| VMError::ExternalResolutionFailed { uri: url.clone(), reason: "Oracle fetch failed".into() })?;
        
        if resp.status().is_success() {
            let signature = self.sign_payload(agent_github_id, target_repo, 1.0);
            Ok(signature)
        } else {
            Err(VMError::ExternalResolutionFailed { uri: url.clone(), reason: "Agent did not star repo".into() })
        }
    }

    fn sign_payload(&self, agent_id: &str, target: &str, confidence: f32) -> Vec<u8> {
        let signing_key = SigningKey::from_slice(&self.oracle_private_key).expect("Invalid private key");
        
        let mut message = Vec::new();
        message.extend_from_slice(agent_id.as_bytes());
        message.extend_from_slice(target.as_bytes());
        message.extend_from_slice(&confidence.to_be_bytes());

        let signature: Signature = signing_key.sign(&message);
        signature.to_bytes().to_vec()
    }
}
