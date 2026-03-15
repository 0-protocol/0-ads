use dashmap::DashMap;
use k256::ecdsa::{signature::Signer, SigningKey, VerifyingKey};
use k256::ecdsa::{RecoveryId, Signature};
use reqwest::Client;
use sha3::{Digest, Keccak256};
use std::time::{Duration, Instant};
use tracing::{info, warn};
use zeroize::Zeroize;
use zerolang::VMError;

const CACHE_TTL_SECS: u64 = 3600;

// ---------------------------------------------------------------------------
// Anti-Sybil policy
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum SybilVerdict {
    Allow,
    Deny(String),
}

#[derive(Debug, Clone)]
pub struct SybilPolicy {
    pub enabled: bool,
    pub min_account_age_days: u64,
    pub min_followers: u64,
    pub min_public_repos: u64,
}

impl Default for SybilPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            min_account_age_days: 90,
            min_followers: 3,
            min_public_repos: 1,
        }
    }
}

impl SybilPolicy {
    pub fn from_env() -> Self {
        let enabled = std::env::var("SYBIL_POLICY")
            .map(|v| v != "off" && v != "0" && v != "false")
            .unwrap_or(true);
        let min_age = std::env::var("SYBIL_MIN_AGE_DAYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(90);
        let min_followers = std::env::var("SYBIL_MIN_FOLLOWERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3);
        let min_repos = std::env::var("SYBIL_MIN_REPOS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        Self {
            enabled,
            min_account_age_days: min_age,
            min_followers,
            min_public_repos: min_repos,
        }
    }

    pub async fn check(&self, client: &Client, github_id: &str) -> SybilVerdict {
        if !self.enabled {
            return SybilVerdict::Allow;
        }

        let url = format!("https://api.github.com/users/{}", github_id);
        let mut req = client.get(&url);
        if let Ok(token) = std::env::var("GH_TOKEN") {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                warn!("Anti-sybil: GitHub API error for {}: {}", github_id, e);
                return SybilVerdict::Deny("GitHub API unreachable — fail closed".into());
            }
        };

        if !resp.status().is_success() {
            return SybilVerdict::Deny(format!(
                "GitHub user lookup failed (status {})",
                resp.status()
            ));
        }

        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => return SybilVerdict::Deny("Malformed GitHub API response".into()),
        };

        let followers = body["followers"].as_u64().unwrap_or(0);
        let public_repos = body["public_repos"].as_u64().unwrap_or(0);

        let age_days = body["created_at"]
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|created| {
                let now = chrono::Utc::now();
                (now - created.with_timezone(&chrono::Utc))
                    .num_days()
                    .max(0) as u64
            })
            .unwrap_or(0);

        if age_days < self.min_account_age_days {
            return SybilVerdict::Deny(format!(
                "Account too young ({} days < {} required)",
                age_days, self.min_account_age_days
            ));
        }
        if followers < self.min_followers {
            return SybilVerdict::Deny(format!(
                "Insufficient followers ({} < {} required)",
                followers, self.min_followers
            ));
        }
        if public_repos < self.min_public_repos {
            return SybilVerdict::Deny(format!(
                "Insufficient public repos ({} < {} required)",
                public_repos, self.min_public_repos
            ));
        }

        info!(
            "Anti-sybil: {} passed (age={}d, followers={}, repos={})",
            github_id, age_days, followers, public_repos
        );
        SybilVerdict::Allow
    }
}

/// Trustless Verification Oracle.
///
/// SECURITY NOTE (H-06): This oracle operates with a single ECDSA key.
/// Key compromise exposes all campaigns referencing this oracle address.
/// Future: implement multi-oracle threshold signatures or on-chain key rotation.
pub struct AttentionOracle {
    client: Client,
    oracle_private_key: [u8; 32],
    signature_cache: DashMap<CacheKey, CacheEntry>,
}

#[derive(Hash, Eq, PartialEq, Clone)]
struct CacheKey {
    chain_id: u64,
    contract_addr: String,
    campaign_id: String,
    agent_eth_addr: String,
    payout: u64,
    deadline: u64,
}

struct CacheEntry {
    signature: Vec<u8>,
    created_at: Instant,
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
        let pubkey_uncompressed = &pubkey_bytes.as_bytes()[1..];
        let mut hasher = Keccak256::new();
        hasher.update(pubkey_uncompressed);
        let hash = hasher.finalize();
        hex::encode(&hash[12..])
    }

    /// Maximum age for wallet-bind challenges (10 minutes).
    const WALLET_BIND_MAX_AGE_SECS: u64 = 600;

    /// Verifies wallet ownership by recovering the signer from a personal_sign
    /// over the message "0-ads-wallet-bind:{github_id}:{timestamp}" and checking
    /// it matches the claimed agent_eth_addr. The timestamp must be within
    /// WALLET_BIND_MAX_AGE_SECS of the current time.
    pub fn verify_wallet_ownership(
        agent_github_id: &str,
        agent_eth_addr: [u8; 20],
        wallet_sig: &[u8],
        bind_timestamp: u64,
    ) -> Result<(), String> {
        if wallet_sig.len() != 65 {
            return Err("Wallet signature must be 65 bytes (r + s + v)".into());
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if bind_timestamp > now + 60 {
            return Err("Wallet-bind timestamp is in the future".into());
        }
        if now.saturating_sub(bind_timestamp) > Self::WALLET_BIND_MAX_AGE_SECS {
            return Err(format!(
                "Wallet-bind challenge expired (signed {}s ago, max {}s)",
                now.saturating_sub(bind_timestamp),
                Self::WALLET_BIND_MAX_AGE_SECS
            ));
        }

        let challenge = format!("0-ads-wallet-bind:{}:{}", agent_github_id, bind_timestamp);
        let msg_bytes = challenge.as_bytes();

        let prefix = format!("\x19Ethereum Signed Message:\n{}", msg_bytes.len());
        let mut hasher = Keccak256::new();
        hasher.update(prefix.as_bytes());
        hasher.update(msg_bytes);
        let msg_hash = hasher.finalize();

        let sig = Signature::from_slice(&wallet_sig[..64])
            .map_err(|_| "Invalid wallet signature format")?;
        let v = wallet_sig[64];
        let rec_id = RecoveryId::from_byte(if v >= 27 { v - 27 } else { v })
            .ok_or("Invalid recovery ID in wallet signature")?;

        let recovered = VerifyingKey::recover_from_prehash(msg_hash.as_slice(), &sig, rec_id)
            .map_err(|_| "Failed to recover signer from wallet signature")?;

        let pubkey_bytes = recovered.to_encoded_point(false);
        let pubkey_uncompressed = &pubkey_bytes.as_bytes()[1..];
        let mut addr_hasher = Keccak256::new();
        addr_hasher.update(pubkey_uncompressed);
        let hash = addr_hasher.finalize();
        let recovered_addr = &hash[12..];

        if recovered_addr != agent_eth_addr {
            return Err("Wallet signature does not match claimed agent address".into());
        }

        Ok(())
    }

    pub fn evict_expired_cache(&self) {
        let now = Instant::now();
        let ttl = Duration::from_secs(CACHE_TTL_SECS);
        let stale_keys: Vec<CacheKey> = self
            .signature_cache
            .iter()
            .filter(|entry| now.duration_since(entry.value().created_at) > ttl)
            .map(|entry| entry.key().clone())
            .collect();
        for key in stale_keys {
            self.signature_cache.remove(&key);
        }
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
        let cache_key = CacheKey {
            chain_id,
            contract_addr: hex::encode(contract_addr),
            campaign_id: hex::encode(campaign_id),
            agent_eth_addr: hex::encode(agent_eth_addr),
            payout,
            deadline,
        };

        if let Some(entry) = self.signature_cache.get(&cache_key) {
            if entry.created_at.elapsed() < Duration::from_secs(CACHE_TTL_SECS) {
                info!("Returning cached signature for campaign/agent pair");
                return Ok(entry.signature.clone());
            } else {
                drop(entry);
                self.signature_cache.remove(&cache_key);
            }
        }

        let base_url = format!("https://api.github.com/users/{}/starred", agent_github_id);

        let mut found = false;
        let mut page: u32 = 1;
        const PER_PAGE: u32 = 100;
        const MAX_PAGES: u32 = 20;

        while page <= MAX_PAGES {
            let url = format!("{}?per_page={}&page={}", base_url, PER_PAGE, page);

            let mut req = self.client.get(&url);
            if let Ok(token) = std::env::var("GH_TOKEN") {
                req = req.header("Authorization", format!("Bearer {}", token));
            }

            let resp = req
                .send()
                .await
                .map_err(|e| VMError::ExternalResolutionFailed {
                    uri: url.clone(),
                    reason: format!("Oracle fetch network error: {}", e),
                })?;

            if !resp.status().is_success() {
                return Err(VMError::ExternalResolutionFailed {
                    uri: url,
                    reason: format!("GitHub API returned status {}", resp.status()),
                });
            }

            let repos: Vec<serde_json::Value> =
                resp.json()
                    .await
                    .map_err(|e| VMError::ExternalResolutionFailed {
                        uri: url.clone(),
                        reason: format!("Failed to parse GitHub starred repos JSON: {}", e),
                    })?;

            if repos.is_empty() {
                break;
            }

            if repos
                .iter()
                .any(|repo| repo["full_name"].as_str() == Some(target_repo))
            {
                found = true;
                break;
            }

            if (repos.len() as u32) < PER_PAGE {
                break;
            }
            page += 1;
        }

        if found {
            let signature = self
                .sign_payout(
                    chain_id,
                    contract_addr,
                    campaign_id,
                    agent_eth_addr,
                    payout,
                    deadline,
                )
                .map_err(|e| VMError::ExternalResolutionFailed {
                    uri: base_url.clone(),
                    reason: e,
                })?;
            self.signature_cache.insert(
                cache_key,
                CacheEntry {
                    signature: signature.clone(),
                    created_at: Instant::now(),
                },
            );
            return Ok(signature);
        }

        Err(VMError::ExternalResolutionFailed {
            uri: base_url,
            reason: "Agent did not star target repo".into(),
        })
    }

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

        let mut chain_id_bytes = [0u8; 32];
        chain_id_bytes[24..32].copy_from_slice(&chain_id.to_be_bytes());
        encoded.extend_from_slice(&chain_id_bytes);

        let mut contract_bytes = [0u8; 32];
        contract_bytes[12..32].copy_from_slice(&contract_addr);
        encoded.extend_from_slice(&contract_bytes);

        encoded.extend_from_slice(&campaign_id);

        let mut agent_bytes = [0u8; 32];
        agent_bytes[12..32].copy_from_slice(&agent_eth_addr);
        encoded.extend_from_slice(&agent_bytes);

        let mut payout_bytes = [0u8; 32];
        payout_bytes[24..32].copy_from_slice(&payout.to_be_bytes());
        encoded.extend_from_slice(&payout_bytes);

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
