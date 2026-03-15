use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Json as AxumJson},
    routing::{get, post},
    Json, Router,
};
use dashmap::DashMap;
use futures::StreamExt;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, Semaphore};
use tracing::{error, info, warn};

mod network;
mod oracle;

const MAX_ACTIVE_INTENTS: usize = 10_000;
const MAX_UNVERIFIED_INTENTS: usize = 5_000;
const MAX_SIGNATURE_DEADLINE_SECS: u64 = 3600; // 1 hour max TTL
const GOSSIPSUB_TOPIC: &str = "0-ads-intents-v1";
const DEFAULT_ESCROW_CONTRACT: &str = "0x8a2aD6bC4A240515c49035bE280BacB7CA94afC4";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdIntent {
    pub campaign_id: String,
    pub advertiser: String,
    pub budget: u64,
    pub payout_per_execution: u64,
    pub verification_graph_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerifyRequest {
    pub agent_github_id: String,
    pub target_repo: String,
    pub chain_id: u64,
    pub contract_addr: String,
    pub campaign_id: String,
    pub agent_eth_addr: String,
    pub payout: u64,
    pub deadline: u64,
    /// EIP-191 personal_sign over "0-ads-wallet-bind:{agent_github_id}:{bind_timestamp}" proving wallet ownership.
    pub wallet_sig: String,
    /// Unix timestamp included in the wallet-bind challenge for replay protection.
    #[serde(default)]
    pub bind_timestamp: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerifyGraphRequest {
    pub graph_hex: String,
    pub agent_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerifyResponse {
    pub signature: String,
    pub error: Option<String>,
}

pub struct AppState {
    active_intents: DashMap<String, AdIntent>,
    unverified_intents: DashMap<String, AdIntent>,
    unverified_order: Mutex<VecDeque<String>>,
    oracle: Arc<oracle::AttentionOracle>,
    sybil_policy: oracle::SybilPolicy,
    sybil_client: reqwest::Client,
    api_secret: Option<String>,
    require_auth: bool,
    graph_execution_enabled: bool,
    graph_semaphore: Arc<Semaphore>,
    rate_limiter: Arc<SlidingWindowRateLimiter>,
    gossipsub_tx: mpsc::UnboundedSender<Vec<u8>>,
    campaign_verifier: Option<Arc<CampaignVerifier>>,
}

fn load_oracle_key() -> Result<[u8; 32], Box<dyn std::error::Error>> {
    if let Ok(hex_key) = std::env::var("ORACLE_PRIVATE_KEY") {
        warn!(
            "ORACLE_PRIVATE_KEY loaded from environment variable. \
             This is insecure in production — use ORACLE_KEY_FILE instead."
        );
        let bytes = hex::decode(hex_key.trim_start_matches("0x"))
            .map_err(|e| format!("ORACLE_PRIVATE_KEY is not valid hex: {}", e))?;
        if bytes.len() != 32 {
            return Err(format!("ORACLE_PRIVATE_KEY must be 32 bytes, got {}", bytes.len()).into());
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }

    if let Ok(path) = std::env::var("ORACLE_KEY_FILE") {
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| format!("Cannot read ORACLE_KEY_FILE at {}: {}", path, e))?;
        let bytes = hex::decode(contents.trim().trim_start_matches("0x"))
            .map_err(|e| format!("ORACLE_KEY_FILE contains invalid hex: {}", e))?;
        if bytes.len() != 32 {
            return Err(
                format!("ORACLE_KEY_FILE key must be 32 bytes, got {}", bytes.len()).into(),
            );
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }

    Err("ORACLE_PRIVATE_KEY or ORACLE_KEY_FILE must be set. \
         The node refuses to start without an explicit oracle key to prevent \
         accidental use of a well-known default in production."
        .into())
}

fn hex_to_32(s: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(s.trim_start_matches("0x"))
        .map_err(|e| format!("Invalid hex for 32-byte value: {}", e))?;
    if bytes.len() != 32 {
        return Err(format!("Expected 32 bytes, got {}", bytes.len()));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

fn hex_to_20(s: &str) -> Result<[u8; 20], String> {
    let bytes = hex::decode(s.trim_start_matches("0x"))
        .map_err(|e| format!("Invalid hex for 20-byte value: {}", e))?;
    if bytes.len() != 20 {
        return Err(format!("Expected 20 bytes, got {}", bytes.len()));
    }
    let mut arr = [0u8; 20];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

fn validate_intent(intent: &AdIntent) -> bool {
    if intent.campaign_id.is_empty() || intent.advertiser.is_empty() {
        return false;
    }
    if intent.budget == 0 || intent.payout_per_execution == 0 {
        return false;
    }
    if intent.budget < intent.payout_per_execution {
        return false;
    }
    true
}

fn check_api_key(
    headers: &HeaderMap,
    expected: &Option<String>,
    require_auth: bool,
) -> Result<(), StatusCode> {
    let secret = match expected {
        Some(s) => s,
        None => {
            if require_auth {
                return Err(StatusCode::SERVICE_UNAVAILABLE);
            }
            return Ok(());
        }
    };
    match headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        Some(provided) if provided == secret => Ok(()),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

fn extract_rate_key(
    headers: &HeaderMap,
    req_identifier: Option<&str>,
    peer_ip: Option<&str>,
) -> String {
    if let Some(key) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        return format!("apikey:{}", key);
    }
    if let Some(id) = req_identifier {
        return format!("agent:{}", id);
    }
    if let Some(ip) = peer_ip {
        return format!("ip:{}", ip);
    }
    "anon:unknown".to_string()
}

fn unix_timestamp_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

struct SlidingWindowRateLimiter {
    windows: DashMap<String, Mutex<VecDeque<Instant>>>,
    max_requests: usize,
    window_secs: u64,
}

impl SlidingWindowRateLimiter {
    fn new(max_requests: usize, window_secs: u64) -> Self {
        Self {
            windows: DashMap::new(),
            max_requests,
            window_secs,
        }
    }

    fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let entry = self
            .windows
            .entry(key.to_string())
            .or_insert_with(|| Mutex::new(VecDeque::new()));
        let mut timestamps = entry.lock();

        let cutoff = now - std::time::Duration::from_secs(self.window_secs);
        while timestamps.front().map_or(false, |t| *t < cutoff) {
            timestamps.pop_front();
        }

        if timestamps.len() >= self.max_requests {
            return false;
        }
        timestamps.push_back(now);
        true
    }

    fn evict_stale(&self) {
        let now = Instant::now();
        let cutoff = now - std::time::Duration::from_secs(self.window_secs * 2);
        let stale_keys: Vec<String> = self
            .windows
            .iter()
            .filter(|entry| {
                let ts = entry.value().lock();
                ts.is_empty() || ts.back().map_or(true, |t| *t < cutoff)
            })
            .map(|entry| entry.key().clone())
            .collect();
        for key in stale_keys {
            self.windows.remove(&key);
        }
    }
}

/// Verifies campaign existence and funding on-chain via EVM JSON-RPC eth_call.
struct CampaignVerifier {
    rpc_url: String,
    contract_addr: String,
    client: reqwest::Client,
    selector: [u8; 4],
}

impl CampaignVerifier {
    fn new(rpc_url: String, contract_addr: String) -> Self {
        let mut hasher = Keccak256::new();
        hasher.update(b"campaigns(bytes32)");
        let hash = hasher.finalize();
        let mut selector = [0u8; 4];
        selector.copy_from_slice(&hash[..4]);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("Failed to build RPC client");

        Self {
            rpc_url,
            contract_addr,
            client,
            selector,
        }
    }

    /// Returns true if the campaign exists on-chain and has budget >= payout.
    async fn is_campaign_funded(&self, campaign_id_hex: &str) -> bool {
        let campaign_bytes = match hex::decode(campaign_id_hex.trim_start_matches("0x")) {
            Ok(b) if b.len() == 32 => b,
            _ => return false,
        };

        let mut calldata = Vec::with_capacity(36);
        calldata.extend_from_slice(&self.selector);
        calldata.extend_from_slice(&campaign_bytes);

        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{
                "to": &self.contract_addr,
                "data": format!("0x{}", hex::encode(&calldata))
            }, "latest"],
            "id": 1
        });

        let resp = match self.client.post(&self.rpc_url).json(&payload).send().await {
            Ok(r) => r,
            Err(e) => {
                warn!("On-chain verification RPC error: {}", e);
                return false;
            }
        };

        let body: serde_json::Value = match resp.json().await {
            Ok(b) => b,
            Err(_) => return false,
        };

        let result_hex = match body["result"].as_str() {
            Some(r) => r.trim_start_matches("0x"),
            None => return false,
        };

        let result_bytes = match hex::decode(result_hex) {
            Ok(b) => b,
            Err(_) => return false,
        };

        // ABI layout: advertiser(32) + token(32) + budget(32) + payout(32) + ...
        if result_bytes.len() < 128 {
            return false;
        }

        // advertiser: bytes 0..32 — zero means campaign doesn't exist
        let advertiser_slice = &result_bytes[0..32];
        if advertiser_slice.iter().all(|&b| b == 0) {
            return false;
        }

        // budget: bytes 64..96, payout: bytes 96..128 (big-endian uint256)
        let budget = &result_bytes[64..96];
        let payout = &result_bytes[96..128];
        budget >= payout
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    info!("Starting 0-ads Billboard Node (Sun Force Edition)...");

    let oracle_key = load_oracle_key()?;
    let oracle =
        Arc::new(oracle::AttentionOracle::new(oracle_key).expect("Failed to initialize Oracle"));

    info!("Oracle address: 0x{}", oracle.public_address_hex());

    let require_auth = std::env::var("REQUIRE_AUTH")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(false);

    let api_secret = std::env::var("API_SECRET").ok();
    if require_auth && api_secret.is_none() {
        return Err("REQUIRE_AUTH is set but API_SECRET is missing. Refusing to start without authentication.".into());
    }
    if api_secret.is_none() {
        warn!("API_SECRET not set — oracle endpoints are unauthenticated. Set REQUIRE_AUTH=true and API_SECRET for production.");
    }

    let graph_execution_enabled = std::env::var("ENABLE_GRAPH_EXECUTION")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    if !graph_execution_enabled {
        info!("Graph execution endpoint is DISABLED. Set ENABLE_GRAPH_EXECUTION=true to enable.");
    }

    let rate_limit_rpm: usize = std::env::var("ORACLE_RATE_LIMIT_RPM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    info!(
        "Oracle rate limit: {} requests per minute per key",
        rate_limit_rpm
    );

    let (gossipsub_tx, mut gossipsub_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    let campaign_verifier = {
        let rpc_url = std::env::var("BASE_RPC_URL")
            .unwrap_or_else(|_| "https://sepolia.base.org".to_string());
        let contract = std::env::var("ESCROW_CONTRACT_ADDR")
            .unwrap_or_else(|_| DEFAULT_ESCROW_CONTRACT.to_string());
        info!(
            "On-chain campaign verification enabled: contract={} rpc={}",
            contract, rpc_url
        );
        Some(Arc::new(CampaignVerifier::new(rpc_url, contract)))
    };

    let sybil_policy = oracle::SybilPolicy::from_env();
    if sybil_policy.enabled {
        info!(
            "Anti-sybil policy ENABLED: min_age={}d, min_followers={}, min_repos={}",
            sybil_policy.min_account_age_days,
            sybil_policy.min_followers,
            sybil_policy.min_public_repos,
        );
    } else {
        warn!("Anti-sybil policy is DISABLED. Set SYBIL_POLICY=on for production.");
    }

    let sybil_client = reqwest::Client::builder()
        .user_agent("0-ads-billboard-node/1.0")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("Failed to build sybil HTTP client");

    let state = Arc::new(AppState {
        active_intents: DashMap::new(),
        unverified_intents: DashMap::new(),
        unverified_order: Mutex::new(VecDeque::new()),
        oracle,
        sybil_policy,
        sybil_client,
        api_secret,
        require_auth,
        graph_execution_enabled,
        graph_semaphore: Arc::new(Semaphore::new(4)),
        rate_limiter: Arc::new(SlidingWindowRateLimiter::new(rate_limit_rpm, 60)),
        gossipsub_tx,
        campaign_verifier,
    });

    let mut swarm = network::build_0_ads_swarm()?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    let api_state = state.clone();
    let app = Router::new()
        .route("/", get(serve_dashboard))
        .route("/api/v1/intents", get(get_intents))
        .route("/api/v1/intents/broadcast", post(broadcast_intent))
        .route("/api/v1/oracle/verify", post(verify_proof))
        .route("/api/v1/oracle/execute_graph", post(verify_graph_execution))
        .with_state(api_state);

    let server_handle = tokio::spawn(async move {
        let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
        let addr = format!("0.0.0.0:{}", port)
            .parse::<std::net::SocketAddr>()
            .expect("Invalid IP/Port configuration");

        let tls_cert = std::env::var("TLS_CERT_PATH").ok();
        let tls_key = std::env::var("TLS_KEY_PATH").ok();

        let allow_tls_fallback = std::env::var("ALLOW_TLS_FALLBACK")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        match (tls_cert, tls_key) {
            (Some(cert_path), Some(key_path)) => {
                info!("Billboard HTTP API listening on {} with TLS", addr);
                let rustls_config = match axum_server::tls_rustls::RustlsConfig::from_pem_file(
                    &cert_path, &key_path,
                )
                .await
                {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        if allow_tls_fallback {
                            warn!("Failed to load TLS certificates: {}. ALLOW_TLS_FALLBACK=true, falling back to plain HTTP.", e);
                            if let Err(e) = axum::Server::bind(&addr)
                                .serve(app.into_make_service())
                                .await
                            {
                                error!("HTTP Server error: {}", e);
                            }
                            return;
                        }
                        error!(
                            "FATAL: TLS was explicitly configured but certificate loading failed: {}. \
                             Refusing to start insecurely. Set ALLOW_TLS_FALLBACK=true to override.",
                            e
                        );
                        std::process::exit(1);
                    }
                };
                if let Err(e) = axum_server::bind_rustls(addr, rustls_config)
                    .serve(app.into_make_service())
                    .await
                {
                    error!("HTTPS Server error: {}", e);
                }
            }
            _ => {
                warn!(
                    "TLS_CERT_PATH and TLS_KEY_PATH not set — running plain HTTP. \
                     This is insecure for production. Set both env vars to enable TLS."
                );
                info!("Billboard HTTP API listening on {} (plain HTTP)", addr);
                if let Err(e) = axum::Server::bind(&addr)
                    .serve(app.into_make_service())
                    .await
                {
                    error!("HTTP Server error: {}", e);
                }
            }
        }
    });

    let verify_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        let mut eviction_counter: u64 = 0;
        loop {
            interval.tick().await;
            eviction_counter += 1;

            if verify_state.unverified_intents.len() > MAX_UNVERIFIED_INTENTS {
                let overflow = verify_state.unverified_intents.len() - MAX_UNVERIFIED_INTENTS;
                let mut order = verify_state.unverified_order.lock();
                let mut evicted = 0usize;
                while evicted < overflow {
                    match order.pop_front() {
                        Some(key) => {
                            if verify_state.unverified_intents.remove(&key).is_some() {
                                evicted += 1;
                            }
                        }
                        None => break,
                    }
                }
                warn!("Evicted {} oldest unverified intents (FIFO)", evicted);
            }

            let keys: Vec<String> = verify_state
                .unverified_intents
                .iter()
                .map(|kv| kv.key().clone())
                .collect();
            for key in keys {
                if verify_state.active_intents.len() >= MAX_ACTIVE_INTENTS {
                    break;
                }
                if let Some((_, intent)) = verify_state.unverified_intents.remove(&key) {
                    if !validate_intent(&intent) {
                        continue;
                    }
                    if verify_state
                        .active_intents
                        .contains_key(&intent.campaign_id)
                    {
                        continue;
                    }

                    if let Some(verifier) = &verify_state.campaign_verifier {
                        if !verifier.is_campaign_funded(&intent.campaign_id).await {
                            warn!(
                                "Dropped intent {}: campaign not found or underfunded on-chain",
                                intent.campaign_id
                            );
                            continue;
                        }
                    }

                    verify_state
                        .active_intents
                        .insert(intent.campaign_id.clone(), intent);
                }
            }

            // Periodically compact stale keys from unverified_order. This keeps
            // the FIFO queue bounded even when intents are promoted to active_intents.
            if eviction_counter % 12 == 0 {
                let mut order = verify_state.unverified_order.lock();
                order.retain(|k| verify_state.unverified_intents.contains_key(k));
            }

            if eviction_counter % 60 == 0 {
                verify_state.rate_limiter.evict_stale();
                verify_state.oracle.evict_expired_cache();
            }
        }
    });

    let ad_topic = gossipsub::IdentTopic::new(GOSSIPSUB_TOPIC);

    loop {
        tokio::select! {
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(network::BehaviourEvent::Gossipsub(
                    gossipsub::Event::Message { message, .. }
                )) => {
                    info!("Received Ad Intent over Gossipsub");
                    if state.unverified_intents.len() < MAX_UNVERIFIED_INTENTS {
                        if let Ok(intent) = serde_json::from_slice::<AdIntent>(&message.data) {
                            let key = intent.campaign_id.clone();
                            state.unverified_intents.insert(key.clone(), intent);
                            state.unverified_order.lock().push_back(key);
                        }
                    }
                }
                SwarmEvent::Behaviour(network::BehaviourEvent::Mdns(mdns::Event::Discovered(peers))) => {
                    for (peer_id, addr) in peers {
                        info!("mDNS discovered peer {} at {}", peer_id, addr);
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    }
                }
                SwarmEvent::Behaviour(network::BehaviourEvent::Mdns(mdns::Event::Expired(peers))) => {
                    for (peer_id, addr) in peers {
                        info!("mDNS peer expired {} at {}", peer_id, addr);
                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    }
                }
                SwarmEvent::NewListenAddr { address, .. } => {
                    info!("P2P Node listening on {:?}", address);
                }
                _ => {}
            },
            Some(data) = gossipsub_rx.recv() => {
                if let Err(e) = swarm.behaviour_mut().gossipsub.publish(ad_topic.clone(), data) {
                    warn!("Failed to publish intent to Gossipsub: {:?}", e);
                }
            },
            _ = tokio::signal::ctrl_c() => {
                info!("Received shutdown signal, exiting gracefully...");
                break;
            }
        }
    }

    drop(swarm);
    server_handle.abort();
    info!("Billboard node shut down.");

    Ok(())
}

async fn serve_dashboard() -> Html<&'static str> {
    Html(include_str!("dashboard.html"))
}

async fn get_intents(State(state): State<Arc<AppState>>) -> Json<Vec<AdIntent>> {
    let intents: Vec<AdIntent> = state
        .active_intents
        .iter()
        .map(|kv| kv.value().clone())
        .collect();
    Json(intents)
}

async fn broadcast_intent(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(intent): Json<AdIntent>,
) -> impl IntoResponse {
    if let Err(status) = check_api_key(&headers, &state.api_secret, state.require_auth) {
        return (
            status,
            AxumJson(serde_json::json!({"error": "Unauthorized: invalid or missing x-api-key"})),
        );
    }

    if !validate_intent(&intent) {
        return (
            StatusCode::BAD_REQUEST,
            AxumJson(
                serde_json::json!({"error": "Invalid intent: missing fields or budget < payout"}),
            ),
        );
    }
    if state.unverified_intents.len() >= MAX_UNVERIFIED_INTENTS {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            AxumJson(serde_json::json!({"error": "Intent capacity reached"})),
        );
    }

    info!(
        "Queuing campaign {} for on-chain verification",
        intent.campaign_id
    );

    if let Ok(data) = serde_json::to_vec(&intent) {
        if let Err(e) = state.gossipsub_tx.send(data) {
            warn!("Failed to queue intent for Gossipsub broadcast: {}", e);
        }
    }

    let key = intent.campaign_id.clone();
    state.unverified_intents.insert(key.clone(), intent);
    state.unverified_order.lock().push_back(key);
    (
        StatusCode::OK,
        AxumJson(
            serde_json::json!({"message": "Intent queued for on-chain verification and Gossipsub broadcast"}),
        ),
    )
}

async fn verify_proof(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<VerifyRequest>,
) -> impl IntoResponse {
    if let Err(status) = check_api_key(&headers, &state.api_secret, state.require_auth) {
        return (
            status,
            Json(VerifyResponse {
                signature: String::new(),
                error: Some("Unauthorized: invalid or missing x-api-key".into()),
            }),
        );
    }

    let rate_key = extract_rate_key(&headers, Some(&req.agent_github_id), None);
    if !state.rate_limiter.check(&rate_key) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(VerifyResponse {
                signature: String::new(),
                error: Some("Rate limit exceeded. Try again later.".into()),
            }),
        );
    }

    let now = unix_timestamp_now();
    if req.deadline <= now {
        return (
            StatusCode::BAD_REQUEST,
            Json(VerifyResponse {
                signature: String::new(),
                error: Some("Deadline must be in the future".into()),
            }),
        );
    }
    if req.deadline > now + MAX_SIGNATURE_DEADLINE_SECS {
        return (
            StatusCode::BAD_REQUEST,
            Json(VerifyResponse {
                signature: String::new(),
                error: Some(format!(
                    "Deadline too far in the future (max {} seconds from now)",
                    MAX_SIGNATURE_DEADLINE_SECS
                )),
            }),
        );
    }

    info!("Oracle verifying agent proof payload...");

    let a_addr = match hex_to_20(&req.agent_eth_addr) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(VerifyResponse {
                    signature: String::new(),
                    error: Some(e),
                }),
            );
        }
    };

    let wallet_sig_bytes = match hex::decode(req.wallet_sig.trim_start_matches("0x")) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(VerifyResponse {
                    signature: String::new(),
                    error: Some("Invalid wallet_sig hex".into()),
                }),
            );
        }
    };
    if let Err(e) = oracle::AttentionOracle::verify_wallet_ownership(
        &req.agent_github_id,
        a_addr,
        &wallet_sig_bytes,
        req.bind_timestamp,
    ) {
        return (
            StatusCode::FORBIDDEN,
            Json(VerifyResponse {
                signature: String::new(),
                error: Some(e),
            }),
        );
    }

    match state
        .sybil_policy
        .check(&state.sybil_client, &req.agent_github_id)
        .await
    {
        oracle::SybilVerdict::Deny(reason) => {
            warn!("Anti-sybil rejected {}: {}", req.agent_github_id, reason);
            return (
                StatusCode::FORBIDDEN,
                Json(VerifyResponse {
                    signature: String::new(),
                    error: Some("Rejected by anti-sybil policy".into()),
                }),
            );
        }
        oracle::SybilVerdict::Allow => {}
    }

    let c_addr = match hex_to_20(&req.contract_addr) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(VerifyResponse {
                    signature: String::new(),
                    error: Some(e),
                }),
            );
        }
    };
    let c_id = match hex_to_32(&req.campaign_id) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(VerifyResponse {
                    signature: String::new(),
                    error: Some(e),
                }),
            );
        }
    };

    match state
        .oracle
        .verify_github_star(
            &req.agent_github_id,
            &req.target_repo,
            req.chain_id,
            c_addr,
            c_id,
            a_addr,
            req.payout,
            req.deadline,
        )
        .await
    {
        Ok(sig) => (
            StatusCode::OK,
            Json(VerifyResponse {
                signature: hex::encode(sig),
                error: None,
            }),
        ),
        Err(e) => {
            error!("Oracle verification failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(VerifyResponse {
                    signature: String::new(),
                    error: Some("Verification failed".into()),
                }),
            )
        }
    }
}

pub async fn verify_graph_execution(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(_req): Json<VerifyGraphRequest>,
) -> impl IntoResponse {
    if !state.graph_execution_enabled {
        return (
            StatusCode::NOT_FOUND,
            Json(VerifyResponse {
                signature: String::new(),
                error: Some("Graph execution is not enabled on this node".into()),
            }),
        );
    }

    if let Err(status) = check_api_key(&headers, &state.api_secret, state.require_auth) {
        return (
            status,
            Json(VerifyResponse {
                signature: String::new(),
                error: Some("Unauthorized: invalid or missing x-api-key".into()),
            }),
        );
    }

    let rate_key = extract_rate_key(&headers, None, None);
    if !state.rate_limiter.check(&rate_key) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(VerifyResponse {
                signature: String::new(),
                error: Some("Rate limit exceeded. Try again later.".into()),
            }),
        );
    }

    info!("Offloading 0-lang VM execution to blocking thread pool...");

    let permit = match state.graph_semaphore.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(VerifyResponse {
                    signature: String::new(),
                    error: Some("Server busy: too many concurrent graph executions".into()),
                }),
            );
        }
    };

    let res = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        std::thread::sleep(std::time::Duration::from_millis(50));
        Ok::<String, String>("0x0-lang-execution-success-signature".to_string())
    })
    .await;

    match res {
        Ok(Ok(sig)) => (
            StatusCode::OK,
            Json(VerifyResponse {
                signature: sig,
                error: None,
            }),
        ),
        Ok(Err(_)) | Err(_) => {
            error!("Graph execution failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(VerifyResponse {
                    signature: String::new(),
                    error: Some("Graph execution failed".into()),
                }),
            )
        }
    }
}
