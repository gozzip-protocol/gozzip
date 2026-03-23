//! Node configuration loaded from TOML.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Top-level node configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Protocol parameters (shared with simulator).
    #[serde(default)]
    pub protocol: ProtocolConfig,

    /// iroh transport settings.
    #[serde(default)]
    pub iroh: IrohConfig,

    /// Karma system settings.
    #[serde(default)]
    pub karma: KarmaConfig,
}

impl NodeConfig {
    /// Load config from a TOML file, falling back to defaults if not found.
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let contents = std::fs::read_to_string(path)
                .with_context(|| format!("reading config from {}", path.display()))?;
            toml::from_str(&contents)
                .with_context(|| format!("parsing config from {}", path.display()))
        } else {
            tracing::warn!(
                path = %path.display(),
                "Config file not found, using defaults"
            );
            Ok(Self::default())
        }
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            protocol: ProtocolConfig::default(),
            iroh: IrohConfig::default(),
            karma: KarmaConfig::default(),
        }
    }
}

/// Protocol parameters matching the simulator's ProtocolConfig.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    /// Target number of active storage pacts.
    #[serde(default = "default_pacts")]
    pub pacts_default: u32,

    /// Target number of standby pacts.
    #[serde(default = "default_standby")]
    pub pacts_standby: u32,

    /// Volume balance tolerance for pact formation (0.0-1.0).
    #[serde(default = "default_tolerance")]
    pub volume_tolerance: f64,

    /// TTL for gossip request forwarding.
    #[serde(default = "default_ttl")]
    pub ttl: u8,

    /// Number of peers to forward gossip requests to.
    #[serde(default = "default_fanout")]
    pub gossip_fanout: u32,

    /// Storage challenges per day per pact partner.
    #[serde(default = "default_challenge_freq")]
    pub challenge_freq_per_day: u32,

    /// Minimum account age (days) before pact formation.
    #[serde(default = "default_min_age")]
    pub min_account_age_days: u32,

    /// Default storage capacity in megabytes.
    #[serde(default = "default_storage_mb")]
    pub default_storage_capacity_mb: u64,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            pacts_default: 20,
            pacts_standby: 3,
            volume_tolerance: 0.30,
            ttl: 3,
            gossip_fanout: 8,
            challenge_freq_per_day: 1,
            min_account_age_days: 7,
            default_storage_capacity_mb: 1024,
        }
    }
}

/// iroh transport configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrohConfig {
    /// Bootstrap peer identifiers.
    #[serde(default)]
    pub bootstrap_peers: Vec<String>,

    /// Path to the Ed25519 identity key file.
    #[serde(default = "default_key_file")]
    pub key_file: String,

    /// Port to bind the iroh endpoint on. 0 = auto-select.
    #[serde(default)]
    pub bind_port: u16,

    /// Gossip topic version string (determines TopicId).
    #[serde(default = "default_gossip_topic")]
    pub gossip_topic: String,

    /// Maximum concurrent connections.
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,

    /// Gossip batch window in milliseconds for privacy shuffling.
    /// 0 = immediate forwarding (no shuffling). Default 150ms.
    #[serde(default = "default_gossip_batch_ms")]
    pub gossip_batch_ms: u64,
}

impl Default for IrohConfig {
    fn default() -> Self {
        Self {
            bootstrap_peers: Vec::new(),
            key_file: default_key_file(),
            bind_port: 0,
            gossip_topic: default_gossip_topic(),
            max_connections: 256,
            gossip_batch_ms: default_gossip_batch_ms(),
        }
    }
}

/// Karma system configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KarmaConfig {
    /// Whether the karma system is enabled.
    #[serde(default)]
    pub enabled: bool,
}

impl Default for KarmaConfig {
    fn default() -> Self {
        Self { enabled: false }
    }
}

fn default_pacts() -> u32 { 20 }
fn default_standby() -> u32 { 3 }
fn default_tolerance() -> f64 { 0.30 }
fn default_ttl() -> u8 { 3 }
fn default_fanout() -> u32 { 8 }
fn default_challenge_freq() -> u32 { 1 }
fn default_min_age() -> u32 { 7 }
fn default_storage_mb() -> u64 { 1024 }
fn default_key_file() -> String { "~/.gozzip/identity.key".to_string() }
fn default_gossip_topic() -> String { "gozzip-v1".to_string() }
fn default_max_connections() -> u32 { 256 }
fn default_gossip_batch_ms() -> u64 { 150 }
