//! Blob storage and transfer — scaffold for iroh-blobs integration.
//!
//! iroh-blobs provides content-addressed blob storage with BLAKE3
//! hashing, chunked transfer, and verification. This module will
//! integrate iroh-blobs when a compatible version is available.
//!
//! Current status: Type definitions and validation only.
//! TODO: Add iroh-blobs dependency when a version compatible with iroh 0.97 is released.
//!       As of 2026-03, iroh-blobs is at 0.99.0 which targets a newer iroh version.

use gozzip_types::{BlobRef, Hash};
use tracing::warn;

/// Blob store configuration.
pub struct BlobConfig {
    /// Maximum blob size to accept (bytes). Default 50 MiB.
    pub max_blob_size: u64,
    /// Directory for persistent blob storage.
    pub storage_path: std::path::PathBuf,
}

impl Default for BlobConfig {
    fn default() -> Self {
        Self {
            max_blob_size: 50 * 1024 * 1024,
            storage_path: std::path::PathBuf::from("~/.gozzip/blobs"),
        }
    }
}

/// Check if a blob reference is within acceptable size limits.
pub fn validate_blob_ref(blob: &BlobRef, config: &BlobConfig) -> bool {
    if blob.size > config.max_blob_size {
        warn!(
            size = blob.size,
            max = config.max_blob_size,
            "Blob exceeds maximum size"
        );
        return false;
    }
    if blob.mime_type.is_empty() {
        warn!("Blob has empty MIME type");
        return false;
    }
    true
}

/// Convert a gozzip Hash to a hex string for logging.
pub fn hash_to_hex(hash: &Hash) -> String {
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}
