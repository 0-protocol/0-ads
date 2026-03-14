use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use libp2p::{gossipsub, SwarmEvent};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

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

struct AppState {
    // In-memory cache of active campaigns from the Gossipsub network
    active_intents: RwLock<Vec<AdIntent>>,
    // We would inject a channel sender here to broadcast to the Swarm
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    info!("Starting 0-ads Billboard Node...");

    let state = Arc::new(AppState {
        active_intents: RwLock::new(Vec::new()),
    });

    // 1. Initialize P2P Swarm
    let mut swarm = network::build_0_ads_swarm()?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // 2. Start HTTP API for lightweight SDKs (Python agents)
    let api_state = state.clone();
    let app = Router::new()
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

    // 3. Enter P2P Event Loop
    loop {
        tokio::select! {
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(gossipsub::Event::Message { message, .. }) => {
                    info!("Received Ad Intent over Gossipsub: {:?}", String::from_utf8_lossy(&message.data));
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

/// GET /api/v1/intents - SDKs fetch this to find active bounties
async fn get_intents(State(state): State<Arc<AppState>>) -> Json<Vec<AdIntent>> {
    let intents = state.active_intents.read().await;
    Json(intents.clone())
}

/// POST /api/v1/intents/broadcast - Web DApps or advertisers post here to flood the P2P network
async fn broadcast_intent(State(_state): State<Arc<AppState>>, Json(intent): Json<AdIntent>) -> Json<&'static str> {
    // In reality, this sends the intent to the `swarm` to be gossiped to all other Billboard nodes.
    info!("Broadcasting campaign {} to P2P network", intent.campaign_id);
    Json("Intent Broadcasted to 0-ads Gossipsub network")
}

/// POST /api/v1/oracle/verify - Agents submit proof of execution (e.g. Moltbook URL)
async fn verify_proof(State(_state): State<Arc<AppState>>, Json(_proof): Json<serde_json::Value>) -> Json<&'static str> {
    // Triggers `oracle::AttentionOracle::verify_github_star`
    // Signs the success payload and returns it to the Agent for the Smart Contract.
    info!("Oracle verifying agent proof payload...");
    Json("0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20") // Mocked secp256k1 signature
}
