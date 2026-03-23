//! Gozzip network node — real P2P implementation using iroh transport.
//!
//! This binary implements the Gozzip protocol over iroh's QUIC-based
//! peer-to-peer networking with gossip-based message propagation.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use gozzip_types::WireMessage;
use tracing::info;

mod config;
mod discovery;
mod net;

#[derive(Parser, Debug)]
#[command(name = "gozzip-node", about = "Gozzip P2P network node")]
struct Cli {
    /// Path to TOML config file.
    #[arg(long, default_value = "config/node.toml")]
    config: PathBuf,

    /// Path to identity key file (generated if missing).
    #[arg(long)]
    key_file: Option<PathBuf>,

    /// Port to bind the iroh endpoint on (0 = auto-select).
    #[arg(long, default_value_t = 0)]
    port: u16,

    /// Bootstrap peer addresses (can be repeated).
    #[arg(long)]
    bootstrap: Vec<String>,

    /// Run with ephemeral identity (no persistence).
    #[arg(long)]
    ephemeral: bool,

    /// Log level filter.
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(&cli.log_level)
        .init();

    // Load config
    let node_config = config::NodeConfig::load(&cli.config)?;

    // Resolve key file path
    let key_path = cli
        .key_file
        .or_else(|| {
            let expanded = shellexpand(&node_config.iroh.key_file);
            Some(PathBuf::from(expanded))
        })
        .unwrap();

    // Load or generate identity
    let identity = if cli.ephemeral {
        info!("Using ephemeral identity (not persisted)");
        net::identity::NodeIdentity::ephemeral()
    } else {
        net::identity::NodeIdentity::load_or_generate(&key_path)?
    };

    info!(
        public_key = %identity.public_key(),
        "Node identity ready"
    );

    // Initialize transport layer
    let transport_config = net::transport::TransportConfig {
        bind_port: if cli.port != 0 {
            cli.port
        } else {
            node_config.iroh.bind_port
        },
        bootstrap_peers: if cli.bootstrap.is_empty() {
            node_config.iroh.bootstrap_peers.clone()
        } else {
            cli.bootstrap.clone()
        },
    };

    let transport = net::transport::TransportLayer::init(identity, transport_config).await?;

    info!(
        endpoint_id = %transport.public_key(),
        "iroh endpoint bound"
    );

    // Start gossip overlay
    let gossip_topic = net::gossip::gozzip_topic_id(&node_config.iroh.gossip_topic);
    info!(?gossip_topic, "Joining gossip topic");

    let (outbound_tx, mut inbound_rx) = transport.start_gossip(gossip_topic).await?;

    info!("Gossip overlay active — listening for peers");

    // Main event loop
    tokio::select! {
        _ = async {
            while let Some((from, msg)) = inbound_rx.recv().await {
                info!(
                    from = hex_short(&from),
                    msg_type = wire_type_name(&msg),
                    "Received message"
                );
            }
        } => {}
        _ = tokio::signal::ctrl_c() => {
            info!("Shutdown signal received");
        }
    }

    // Graceful shutdown
    info!("Shutting down...");
    drop(outbound_tx);
    transport.shutdown().await?;
    info!("Shutdown complete");

    Ok(())
}

/// Simple shell expansion for ~ in paths.
fn shellexpand(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}/{}", home, rest);
        }
    }
    path.to_string()
}

/// Get a human-readable type name for a WireMessage variant.
fn wire_type_name(msg: &WireMessage) -> &'static str {
    match msg {
        WireMessage::Publish(_) => "Publish",
        WireMessage::RequestData { .. } => "RequestData",
        WireMessage::DeliverEvents { .. } => "DeliverEvents",
        WireMessage::PactRequest { .. } => "PactRequest",
        WireMessage::PactOffer { .. } => "PactOffer",
        WireMessage::PactAccept { .. } => "PactAccept",
        WireMessage::PactDrop { .. } => "PactDrop",
        WireMessage::Challenge { .. } => "Challenge",
        WireMessage::ChallengeResponse { .. } => "ChallengeResponse",
        WireMessage::EncryptedDm { .. } => "EncryptedDm",
        WireMessage::ChannelBroadcast { .. } => "ChannelBroadcast",
        WireMessage::Announce { .. } => "Announce",
        WireMessage::BlobRequest { .. } => "BlobRequest",
    }
}

/// Format the first 8 bytes of a public key as hex for logging.
fn hex_short(bytes: &[u8; 32]) -> String {
    bytes[..8].iter().map(|b| format!("{:02x}", b)).collect()
}
