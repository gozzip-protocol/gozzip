//! Node identity management.
//!
//! Each Gozzip node has a persistent Ed25519 keypair used by iroh
//! for QUIC connection authentication. This is independent from the
//! Nostr secp256k1 identity — see docs/design/iroh-identity-mapping.md.

use std::path::Path;

use iroh::SecretKey;
use tracing::info;

use super::NetError;

/// A node's cryptographic identity backed by an iroh Ed25519 keypair.
pub struct NodeIdentity {
    secret_key: SecretKey,
}

impl NodeIdentity {
    /// Load an identity from a key file, or generate a new one and save it.
    ///
    /// The key file contains the raw 32-byte Ed25519 secret key.
    pub fn load_or_generate(path: &Path) -> Result<Self, NetError> {
        if path.exists() {
            let bytes = std::fs::read(path).map_err(NetError::Io)?;
            if bytes.len() != 32 {
                return Err(NetError::Identity(format!(
                    "key file {} has {} bytes, expected 32",
                    path.display(),
                    bytes.len()
                )));
            }
            let mut key_bytes = [0u8; 32];
            key_bytes.copy_from_slice(&bytes);
            let secret_key = SecretKey::from_bytes(&key_bytes);
            info!(path = %path.display(), "Loaded identity from key file");
            Ok(Self { secret_key })
        } else {
            // Generate a random 32-byte seed using getrandom (OS entropy)
            let secret_key = generate_random_key();
            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(NetError::Io)?;
            }
            std::fs::write(path, secret_key.to_bytes()).map_err(NetError::Io)?;
            info!(
                path = %path.display(),
                public_key = %secret_key.public(),
                "Generated new identity"
            );
            Ok(Self { secret_key })
        }
    }

    /// Generate an ephemeral identity (not persisted to disk).
    pub fn ephemeral() -> Self {
        Self {
            secret_key: generate_random_key(),
        }
    }

    /// The node's public key (iroh EndpointId).
    pub fn public_key(&self) -> iroh::PublicKey {
        self.secret_key.public()
    }

    /// Reference to the secret key (needed for Endpoint construction).
    pub fn secret_key(&self) -> &SecretKey {
        &self.secret_key
    }
}

/// Generate a random Ed25519 secret key using OS entropy.
fn generate_random_key() -> SecretKey {
    let mut bytes = [0u8; 32];
    // Use getrandom for cryptographically secure randomness
    getrandom::fill(&mut bytes).expect("OS entropy source failed");
    SecretKey::from_bytes(&bytes)
}
