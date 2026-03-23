//! Gossip overlay management.
//!
//! Manages topic subscriptions and message serialization for
//! iroh-gossip's epidemic broadcast protocol (HyParView + PlumTree).

use std::time::Duration;

use bytes::Bytes;
use futures_lite::StreamExt;
use gozzip_types::{PubKey, WireMessage};
use iroh::PublicKey;
use iroh_gossip::{net::Gossip, proto::TopicId};
use rand::seq::SliceRandom;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Derive the TopicId for a Gozzip gossip channel from a version string.
///
/// The topic ID is the SHA-256 hash of `"gozzip:{version}"`.
pub fn gozzip_topic_id(version: &str) -> TopicId {
    let hash = Sha256::digest(format!("gozzip:{}", version).as_bytes());
    TopicId::from_bytes(hash.into())
}

/// Derive a per-author TopicId for subscribing to a specific author's events.
pub fn author_topic_id(author: &PubKey) -> TopicId {
    let mut hasher = Sha256::new();
    hasher.update(b"gozzip:author:");
    hasher.update(author);
    TopicId::from_bytes(hasher.finalize().into())
}

/// Configuration for gossip relay behavior.
pub struct GossipConfig {
    /// Batch window duration for outbound message shuffling.
    /// Messages are collected for this duration, shuffled, then broadcast.
    /// Set to Duration::ZERO for immediate forwarding (no privacy enhancement).
    pub batch_window: Duration,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            batch_window: Duration::from_millis(150),
        }
    }
}

/// Start the gossip relay loop.
///
/// Spawns a background task that bridges between the iroh-gossip
/// subscription and application-level mpsc channels.
///
/// Returns (outbound_sender, inbound_receiver).
pub async fn start_gossip_relay(
    gossip: Gossip,
    topic_id: TopicId,
    bootstrap: Vec<PublicKey>,
    config: GossipConfig,
) -> Result<
    (
        mpsc::Sender<WireMessage>,
        mpsc::Receiver<(PubKey, WireMessage)>,
    ),
    super::NetError,
> {
    // Subscribe to the gossip topic
    let topic = gossip
        .subscribe_and_join(topic_id, bootstrap)
        .await
        .map_err(|e| super::NetError::Gossip(e.to_string()))?;

    let (sender, mut receiver) = topic.split();

    // Outbound channel: application → gossip
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<WireMessage>(256);

    // Inbound channel: gossip → application
    let (inbound_tx, inbound_rx) = mpsc::channel::<(PubKey, WireMessage)>(256);

    // Outbound relay task with batch-and-shuffle
    let sender_clone = sender.clone();
    let batch_window = config.batch_window;
    tokio::spawn(async move {
        if batch_window.is_zero() {
            // Immediate forwarding mode (no privacy enhancement)
            while let Some(msg) = outbound_rx.recv().await {
                match postcard::to_allocvec(&msg) {
                    Ok(bytes) => {
                        if let Err(e) = sender_clone.broadcast(Bytes::from(bytes)).await {
                            warn!("Failed to broadcast gossip message: {}", e);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to serialize outbound message: {}", e);
                    }
                }
            }
        } else {
            // Batched forwarding with shuffle
            let mut batch: Vec<Bytes> = Vec::new();
            let mut interval = tokio::time::interval(batch_window);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    msg = outbound_rx.recv() => {
                        match msg {
                            Some(msg) => {
                                match postcard::to_allocvec(&msg) {
                                    Ok(bytes) => batch.push(Bytes::from(bytes)),
                                    Err(e) => warn!("Failed to serialize outbound message: {}", e),
                                }
                            }
                            None => {
                                // Channel closed — flush remaining batch and exit
                                if !batch.is_empty() {
                                    batch.shuffle(&mut rand::thread_rng());
                                    for bytes in batch.drain(..) {
                                        let _ = sender_clone.broadcast(bytes).await;
                                    }
                                }
                                break;
                            }
                        }
                    }
                    _ = interval.tick() => {
                        if !batch.is_empty() {
                            batch.shuffle(&mut rand::thread_rng());
                            debug!(
                                batch_size = batch.len(),
                                "Flushing shuffled gossip batch"
                            );
                            for bytes in batch.drain(..) {
                                if let Err(e) = sender_clone.broadcast(bytes).await {
                                    warn!("Failed to broadcast batched message: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        }
        debug!("Outbound gossip relay shut down");
    });

    // Inbound relay task
    tokio::spawn(async move {
        while let Ok(Some(event)) = receiver.try_next().await {
            match event {
                iroh_gossip::api::Event::Received(msg) => {
                    match postcard::from_bytes::<WireMessage>(&msg.content) {
                        Ok(wire_msg) => {
                            let from: PubKey = *msg.delivered_from.as_bytes();
                            if inbound_tx.send((from, wire_msg)).await.is_err() {
                                debug!("Inbound channel closed");
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to deserialize gossip message: {}", e);
                        }
                    }
                }
                iroh_gossip::api::Event::NeighborUp(peer) => {
                    debug!(peer = %peer, "Gossip neighbor joined");
                }
                iroh_gossip::api::Event::NeighborDown(peer) => {
                    debug!(peer = %peer, "Gossip neighbor left");
                }
                iroh_gossip::api::Event::Lagged => {
                    warn!("Gossip subscription lagged — messages may have been dropped");
                }
            }
        }
        debug!("Inbound gossip relay shut down");
    });

    Ok((outbound_tx, inbound_rx))
}
