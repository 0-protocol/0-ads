use libp2p::{
    gossipsub::{self, MessageAuthenticity, ValidationMode},
    identity, mdns,
    swarm::NetworkBehaviour,
    Multiaddr, PeerId, Swarm,
};
use std::path::PathBuf;
use tracing::{info, warn};

const NODE_KEY_FILE: &str = "node_identity.key";

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "BehaviourEvent")]
pub struct Behaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
}

#[derive(Debug)]
pub enum BehaviourEvent {
    Gossipsub(gossipsub::Event),
    Mdns(mdns::Event),
}

impl From<gossipsub::Event> for BehaviourEvent {
    fn from(value: gossipsub::Event) -> Self {
        Self::Gossipsub(value)
    }
}

impl From<mdns::Event> for BehaviourEvent {
    fn from(value: mdns::Event) -> Self {
        Self::Mdns(value)
    }
}

/// Load or create a persistent Ed25519 identity so the node keeps its PeerId across restarts.
fn load_or_generate_identity() -> identity::Keypair {
    let key_dir = std::env::var("NODE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let key_path = key_dir.join(NODE_KEY_FILE);

    if key_path.exists() {
        match std::fs::read(&key_path) {
            Ok(bytes) => {
                if let Ok(keypair) = identity::Keypair::from_protobuf_encoding(&bytes) {
                    info!(
                        "Loaded persistent node identity from {}",
                        key_path.display()
                    );
                    return keypair;
                }
                warn!("Corrupt node identity file, generating new one");
            }
            Err(e) => warn!("Could not read node key file: {}", e),
        }
    }

    let keypair = identity::Keypair::generate_ed25519();
    if let Ok(encoded) = keypair.to_protobuf_encoding() {
        let _ = std::fs::create_dir_all(&key_dir);
        if let Err(e) = std::fs::write(&key_path, &encoded) {
            warn!("Could not persist node identity: {}", e);
        } else {
            info!(
                "Generated and saved new node identity to {}",
                key_path.display()
            );
        }
    }
    keypair
}

/// Parse bootstrap peer addresses from the BOOTSTRAP_PEERS env var (comma-separated multiaddrs).
fn bootstrap_peers() -> Vec<Multiaddr> {
    std::env::var("BOOTSTRAP_PEERS")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .filter_map(|s| {
            s.trim()
                .parse::<Multiaddr>()
                .map_err(|e| {
                    warn!("Invalid bootstrap peer address '{}': {}", s.trim(), e);
                    e
                })
                .ok()
        })
        .collect()
}

pub fn build_0_ads_swarm() -> Result<Swarm<Behaviour>, Box<dyn std::error::Error>> {
    let local_key = load_or_generate_identity();
    let local_key_clone = local_key.clone();
    let local_peer_id = PeerId::from(local_key.public());
    info!("Node PeerId: {}", local_peer_id);

    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(std::time::Duration::from_secs(1))
        .validation_mode(ValidationMode::Strict)
        .build()
        .expect("Valid config");

    let mut gossipsub = gossipsub::Behaviour::new(
        MessageAuthenticity::Signed(local_key_clone),
        gossipsub_config,
    )
    .expect("Correct configuration");

    let ad_topic = gossipsub::IdentTopic::new("0-ads-intents-v1");
    gossipsub.subscribe(&ad_topic)?;

    let mdns_behaviour = mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)?;

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(local_key)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )?
        .with_behaviour(|_| Behaviour {
            gossipsub,
            mdns: mdns_behaviour,
        })?
        .build();

    let peers = bootstrap_peers();
    if peers.is_empty() {
        warn!(
            "No BOOTSTRAP_PEERS configured — P2P discovery is disabled. \
             Set BOOTSTRAP_PEERS=/ip4/x.x.x.x/tcp/PORT/p2p/PEER_ID for production."
        );
    } else {
        for addr in &peers {
            match swarm.dial(addr.clone()) {
                Ok(_) => info!("Dialing bootstrap peer: {}", addr),
                Err(e) => warn!("Failed to dial bootstrap peer {}: {}", addr, e),
            }
        }
    }

    Ok(swarm)
}
