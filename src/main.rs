use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Json as AxumJson},
    routing::{get, post, any},
    Router,
};
use serde_json::json;
use tracing::{error, info, warn};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    warn!("🚨 V1 CONTRACT PERMANENTLY DEPRECATED AND COMPROMISED 🚨");
    warn!("The legacy 0-ads V1 REST Oracle is officially DEAD due to a hardcoded key leak.");
    warn!("All incoming traffic will receive a 410 Gone or 403 Forbidden response.");
    warn!("Please migrate to the V2 ZK-Native Architecture immediately.");

    let app = Router::new()
        .route("/", get(dashboard))
        .route("/*path", any(deprecated_handler));

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();
    
    info!("Starting V1 Deprecation Tombstone on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn dashboard() -> impl IntoResponse {
    let html = r#"
<!DOCTYPE html>
<html>
<head>
    <title>0-ads V1 Deprecated</title>
    <style>
        body { background: #111; color: #f44336; font-family: monospace; padding: 3rem; text-align: center; }
        h1 { font-size: 3rem; }
        p { font-size: 1.2rem; color: #ccc; }
        .code { background: #222; padding: 1rem; border-radius: 5px; display: inline-block; text-align: left; }
    </style>
</head>
<body>
    <h1>🚨 V1 CONTRACT DEPRECATED 🚨</h1>
    <p>The V1 legacy REST API for 0-ads has been permanently shut down.</p>
    <p>A fatal key compromise forced a hard fork to the Phase 32 ZK-Native architecture.</p>
    <br>
    <div class="code">
        STATUS: 410 GONE<br>
        MIGRATION: USE V2 ZK-PROOFS<br>
        FUNDS: SAFELY WITHDRAWN
    </div>
</body>
</html>
"#;
    Html(html)
}

async fn deprecated_handler() -> impl IntoResponse {
    (
        StatusCode::GONE,
        AxumJson(json!({
            "error": "V1_DEPRECATED",
            "message": "The V1 REST API is permanently shut down. Please use the V2 ZK-Native workflow.",
            "docs": "https://github.com/0-protocol/0-ads"
        }))
    )
}
