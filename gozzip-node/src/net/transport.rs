//! High-level transport layer wrapping iroh Endpoint + Gossip.
//!
//! Provides a unified interface for initializing the networking stack,
//! joining gossip topics, and gracefully shutting down.

use gozzip_types::{PubKey, WireMessage};
use iroh::{protocol::Router, Endpoint, PublicKey};
use iroh_gossip::{net::Gossip, proto::TopicId, ALPN as GOSSIP_ALPN};
use tokio::sync::mpsc;
use tracing::info;

use super::gossip;
use super::identity::NodeIdentity;
use super::NetError;

/// Configuration for the transport layer.
pub struct TransportConfig {
    /// Port to bind the iroh endpoint on. 0 = auto-select.
    pub bind_port: u16,
    /// Bootstrap peer identifiers.
    pub bootstrap_peers: Vec<String>,
}

/// The transport layer manages the iroh Endpoint, Gossip protocol,
/// and protocol Router.
pub struct TransportLayer {
    endpoint: Endpoint,
    gossip: Gossip,
    _router: Router,
    identity: NodeIdentity,
}

impl TransportLayer {
    /// Initialize the full transport stack.
    ///
    /// 1. Creates an iroh Endpoint with the node's Ed25519 identity
    /// 2. Spawns the Gossip protocol handler
    /// 3. Registers protocols on the Router
    /// 4. Binds and starts listening
    pub async fn init(
        identity: NodeIdentity,
        _config: TransportConfig,
    ) -> Result<Self, NetError> {
        // Create iroh endpoint with n0 presets (relay servers + discovery)
        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
            .secret_key(identity.secret_key().clone())
            .bind()
            .await
            .map_err(|e| NetError::Endpoint(anyhow::anyhow!(e)))?;

        // Create gossip protocol handler
        let gossip = Gossip::builder().spawn(endpoint.clone());

        // Build protocol router
        let router = Router::builder(endpoint.clone())
            .accept(GOSSIP_ALPN, gossip.clone())
            .spawn();

        info!("Transport layer initialized");

        Ok(Self {
            endpoint,
            gossip,
            _router: router,
            identity,
        })
    }

    /// Start gossip on a topic and return message channels.
    pub async fn start_gossip(
        &self,
        topic_id: TopicId,
    ) -> Result<
        (
            mpsc::Sender<WireMessage>,
            mpsc::Receiver<(PubKey, WireMessage)>,
        ),
        NetError,
    > {
        // Parse bootstrap peers (for now, empty — peers join via discovery)
        let bootstrap: Vec<PublicKey> = Vec::new();

        gossip::start_gossip_relay(
            self.gossip.clone(),
            topic_id,
            bootstrap,
            gossip::GossipConfig::default(),
        )
        .await
    }

    /// This node's public key (iroh EndpointId).
    pub fn public_key(&self) -> PublicKey {
        self.identity.public_key()
    }

    /// Gracefully shut down the transport layer.
    pub async fn shutdown(self) -> Result<(), NetError> {
        self.endpoint.close().await;
        info!("iroh endpoint closed");
        Ok(())
    }
}
