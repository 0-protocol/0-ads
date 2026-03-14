use axum::{
    extract::State,
    response::Html,
    routing::{get, post},
    Json, Router,
};
use libp2p::{gossipsub, swarm::SwarmEvent};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use futures::StreamExt;
use tokio::sync::RwLock;
use tracing::info;
use rand::RngCore;

mod network;
mod oracle;

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
    // Add required cryptographic binding fields
    pub chain_id: u64,
    pub contract_addr: String, // hex encoded
    pub campaign_id: String,   // hex encoded
    pub agent_eth_addr: String, // hex encoded
    pub payout: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerifyResponse {
    pub signature: String,
    pub error: Option<String>,
}

struct AppState {
    active_intents: RwLock<Vec<AdIntent>>,
    oracle: Arc<oracle::AttentionOracle>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    info!("Starting 0-ads Billboard Node...");

    let mut oracle_key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut oracle_key);
    let oracle = Arc::new(oracle::AttentionOracle::new(oracle_key).expect("Failed to initialize Oracle"));

    let state = Arc::new(AppState {
        active_intents: RwLock::new(Vec::new()),
        oracle,
    });

    let mut swarm = network::build_0_ads_swarm()?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    let api_state = state.clone();
    let app = Router::new()
        .route("/", get(serve_dashboard))
        .route("/api/v1/intents", get(get_intents))
        .route("/api/v1/intents/broadcast", post(broadcast_intent))
        .route("/api/v1/oracle/verify", post(verify_proof))
        .with_state(api_state);

    let server_handle = tokio::spawn(async move {
        let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
        let addr = format!("0.0.0.0:{}", port).parse::<std::net::SocketAddr>().unwrap();
        info!("Billboard HTTP API listening on {}", addr);
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
    });

    loop {
        tokio::select! {
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(gossipsub::Event::Message { message, .. }) => {
                    info!("Received Ad Intent over Gossipsub");
                    if let Ok(intent) = serde_json::from_slice::<AdIntent>(&message.data) {
                        let mut cache = state.active_intents.write().await;
                        cache.push(intent);
                    }
                }
                SwarmEvent::NewListenAddr { address, .. } => {
                    info!("P2P Node listening on {:?}", address);
                }
                _ => {}
            }
        }
    }
}

async fn serve_dashboard() -> Html<&'static str> {
    Html(include_str!("dashboard.html"))
}

async fn get_intents(State(state): State<Arc<AppState>>) -> Json<Vec<AdIntent>> {
    let intents = state.active_intents.read().await;
    Json(intents.clone())
}

async fn broadcast_intent(State(state): State<Arc<AppState>>, Json(intent): Json<AdIntent>) -> Json<&'static str> {
    info!("Broadcasting campaign {} to P2P network", intent.campaign_id);
    let mut cache = state.active_intents.write().await;
    cache.push(intent);
    Json("Intent Broadcasted to 0-ads Gossipsub network")
}

fn hex_to_32(s: &str) -> [u8; 32] {
    let bytes = hex::decode(s.trim_start_matches("0x")).unwrap_or_default();
    let mut arr = [0u8; 32];
    let len = bytes.len().min(32);
    arr[32-len..].copy_from_slice(&bytes[..len]);
    arr
}

fn hex_to_20(s: &str) -> [u8; 20] {
    let bytes = hex::decode(s.trim_start_matches("0x")).unwrap_or_default();
    let mut arr = [0u8; 20];
    let len = bytes.len().min(20);
    arr[20-len..].copy_from_slice(&bytes[..len]);
    arr
}

async fn verify_proof(State(state): State<Arc<AppState>>, Json(req): Json<VerifyRequest>) -> Json<VerifyResponse> {
    info!("Oracle verifying agent proof payload...");
    
    let c_addr = hex_to_20(&req.contract_addr);
    let c_id = hex_to_32(&req.campaign_id);
    let a_addr = hex_to_20(&req.agent_eth_addr);

    match state.oracle.verify_github_star(
        &req.agent_github_id, 
        &req.target_repo,
        req.chain_id,
        c_addr,
        c_id,
        a_addr,
        req.payout
    ).await {
        Ok(sig) => Json(VerifyResponse {
            signature: hex::encode(sig),
            error: None,
        }),
        Err(e) => Json(VerifyResponse {
            signature: "".to_string(),
            error: Some(format!("{:?}", e)),
        }),
    }
}
