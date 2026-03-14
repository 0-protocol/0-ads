use libp2p::core::Transport;
use libp2p::{
    gossipsub::{self, MessageAuthenticity, ValidationMode},
    identity, PeerId, Swarm,
};

/// Phase 3: P2P Gossipsub Network for Attention Intents
/// Agents use this network to receive high-paying Ad Intents without polling central servers.

pub fn build_0_ads_swarm() -> Result<Swarm<gossipsub::Behaviour>, Box<dyn std::error::Error>> {
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());

    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(std::time::Duration::from_secs(1))
        .validation_mode(ValidationMode::Strict)
        .build()
        .expect("Valid config");

    let mut gossipsub = gossipsub::Behaviour::new(
        MessageAuthenticity::Signed(local_key),
        gossipsub_config,
    ).expect("Correct configuration");

    // The topic that brands/advertisers broadcast Ad Intents to.
    let ad_topic = gossipsub::IdentTopic::new("0-ads-intents-v1");
    gossipsub.subscribe(&ad_topic)?;

    // Start listening on all interfaces
    // swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    Ok(Swarm::new(
        libp2p::tcp::tokio::Transport::default().boxed(),
        gossipsub,
        local_peer_id,
        libp2p::swarm::Config::with_tokio_executor(),
    ))
}
