//! Networking layer for the Gozzip node.
//!
//! Wraps iroh's QUIC transport and iroh-gossip's epidemic broadcast
//! to provide peer-to-peer connectivity for the Gozzip protocol.

pub mod blobs;
pub mod gossip;
pub mod identity;
pub mod transport;

/// Errors from the networking layer.
#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("iroh endpoint error: {0}")]
    Endpoint(#[from] anyhow::Error),

    #[error("gossip error: {0}")]
    Gossip(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] postcard::Error),

    #[error("identity error: {0}")]
    Identity(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
