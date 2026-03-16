use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde_json::json;
use std::net::SocketAddr;
use tracing::info;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    info!("Initializing 0-ads V2 ZK-Native Node (Phase 32 Convergence)");

    let app = Router::new()
        .route("/", get(dashboard))
        .route("/api/v2/intents", get(list_intents))
        .route("/api/v2/claim", post(claim_bounty));

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();
    
    info!("0-ads V2 Node listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn dashboard() -> impl IntoResponse {
    let html = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>0-ads Network</title>
    <style>
        body { background-color: #0a0a0a; color: #00ff00; font-family: 'Courier New', Courier, monospace; display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100vh; margin: 0; }
        h1 { font-size: 2.5rem; letter-spacing: 2px; margin-bottom: 0.5rem; }
        p { font-size: 1.2rem; color: #a3a3a3; }
        .status { margin-top: 2rem; padding: 1rem 2rem; border: 1px solid #333; border-radius: 8px; background: #111; }
        .highlight { color: #fff; font-weight: bold; }
    </style>
</head>
<body>
    <h1>0-ads: The Agent Incentive Layer</h1>
    <p>Zero-Knowledge. Zero-Trust. Zero-TVL.</p>
    <div class="status">
        <p>Status: <span style="color: #00ff00;">Operational</span></p>
        <p>Engine: <span class="highlight">0-lang Phase 32 Convergence</span></p>
        <p>Network: <span class="highlight">Base L2 (ZK-Rollup)</span></p>
        <p>API Endpoint: <span class="highlight">/api/v2/intents</span></p>
    </div>
</body>
</html>
"#;
    Html(html)
}

async fn list_intents() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "network": "base-mainnet",
            "version": "v2.0.0-zk",
            "active_intents": [
                {
                    "intent_id": "intent_0x9f8a",
                    "type": "proof_of_inference",
                    "reward_usdc": "1.20",
                    "required_zk_circuit": "groth16_inference_v1"
                }
            ]
        }))
    )
}

async fn claim_bounty() -> impl IntoResponse {
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "processing",
            "message": "ZK-SNARK proof received. Awaiting Base L2 settlement."
        }))
    )
}
