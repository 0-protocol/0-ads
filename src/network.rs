use libp2p::{
    gossipsub::{self, MessageAuthenticity, ValidationMode},
    identity, PeerId, Swarm,
};

pub fn build_0_ads_swarm() -> Result<Swarm<gossipsub::Behaviour>, Box<dyn std::error::Error>> {
    let local_key = identity::Keypair::generate_ed25519();
    let local_key_clone = local_key.clone();
    let local_peer_id = PeerId::from(local_key.public());

    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(std::time::Duration::from_secs(1))
        .validation_mode(ValidationMode::Strict)
        .build()
        .expect("Valid config");

    let mut gossipsub = gossipsub::Behaviour::new(
        MessageAuthenticity::Signed(local_key_clone),
        gossipsub_config,
    ).expect("Correct configuration");

    let ad_topic = gossipsub::IdentTopic::new("0-ads-intents-v1");
    gossipsub.subscribe(&ad_topic)?;

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(local_key)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )?
        .with_behaviour(|_| gossipsub)?
        .build();

    Ok(swarm)
}
