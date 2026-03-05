use crate::types::{EventKind, NodeId};

#[cfg(feature = "nostr-events")]
use std::collections::HashMap;

#[cfg(feature = "nostr-events")]
use nostr::prelude::*;

#[cfg(feature = "nostr-events")]
use sha2::{Digest, Sha256};

// ── NodeRegistry ──────────────────────────────────────────────────

/// Maps simulator NodeIds to deterministic secp256k1 key pairs.
/// When the `nostr-events` feature is disabled, this is a zero-cost no-op.
pub struct NodeRegistry {
    #[cfg(feature = "nostr-events")]
    keys: HashMap<NodeId, Keys>,
    #[cfg(feature = "nostr-events")]
    pubkey_to_id: HashMap<PublicKey, NodeId>,
}

impl NodeRegistry {
    /// Generate deterministic keys for `node_count` nodes.
    ///
    /// Each key is derived from `SHA-256("gozzip-sim-node-" || id.to_le_bytes())`,
    /// giving reproducible key material across runs.
    pub fn generate(node_count: u32) -> Self {
        #[cfg(feature = "nostr-events")]
        {
            let mut keys = HashMap::with_capacity(node_count as usize);
            let mut pubkey_to_id = HashMap::with_capacity(node_count as usize);

            for id in 0..node_count {
                let mut hasher = Sha256::new();
                hasher.update(b"gozzip-sim-node-");
                hasher.update(id.to_le_bytes());
                let hash = hasher.finalize();

                // Use the 32-byte SHA-256 output as a secret key
                let secret_key = SecretKey::from_slice(&hash)
                    .expect("SHA-256 output is valid secp256k1 secret key");
                let node_keys = Keys::new(secret_key);
                pubkey_to_id.insert(node_keys.public_key(), id);
                keys.insert(id, node_keys);
            }

            Self { keys, pubkey_to_id }
        }

        #[cfg(not(feature = "nostr-events"))]
        {
            let _ = node_count;
            Self {}
        }
    }

    /// Get the Keys for a given node ID.
    #[cfg(feature = "nostr-events")]
    pub fn get_keys(&self, id: NodeId) -> Option<&Keys> {
        self.keys.get(&id)
    }

    /// Look up the NodeId for a given public key.
    #[cfg(feature = "nostr-events")]
    #[allow(dead_code)]
    pub fn lookup_id(&self, pubkey: &PublicKey) -> Option<NodeId> {
        self.pubkey_to_id.get(pubkey).copied()
    }

    /// Get the 32-byte secret key for a node (for embedding in NodeState).
    #[cfg(feature = "nostr-events")]
    pub fn get_secret_key_bytes(&self, id: NodeId) -> Option<[u8; 32]> {
        self.keys.get(&id).map(|k| k.secret_key().secret_bytes())
    }

    /// Get the 32-byte secret key for a node (no-op without feature).
    #[cfg(not(feature = "nostr-events"))]
    pub fn get_secret_key_bytes(&self, _id: NodeId) -> Option<[u8; 32]> {
        None
    }
}

// ── Kind mapping ──────────────────────────────────────────────────

/// Map our simulator EventKind to a NIP-01 kind number.
pub fn sim_kind_to_nostr_kind(kind: EventKind) -> u16 {
    match kind {
        EventKind::Note => 1,
        EventKind::Reaction => 7,
        EventKind::Repost => 6,
        EventKind::Dm => 4,
        EventKind::LongForm => 30023,
    }
}

// ── Event creation ────────────────────────────────────────────────

/// Create a signed NIP-01 Nostr event and return its JSON representation.
///
/// Returns `None` when the `nostr-events` feature is disabled or if
/// signing fails.
#[cfg(feature = "nostr-events")]
pub fn create_signed_event(
    registry: &NodeRegistry,
    author: NodeId,
    kind_u16: u16,
    content: &str,
    tags: Vec<Tag>,
    created_at: u64,
) -> Option<String> {
    let keys = registry.get_keys(author)?;
    let mut builder = EventBuilder::new(Kind::from(kind_u16), content);
    if !tags.is_empty() {
        builder = builder.tags(tags);
    }
    builder = builder.custom_created_at(Timestamp::from(created_at));
    let event = builder.sign_with_keys(keys).ok()?;
    serde_json::to_string(&event).ok()
}

#[cfg(not(feature = "nostr-events"))]
pub fn create_signed_event(
    _registry: &NodeRegistry,
    _author: NodeId,
    _kind_u16: u16,
    _content: &str,
    _tags: Vec<()>,
    _created_at: u64,
) -> Option<String> {
    None
}

// ── Pact event builders ──────────────────────────────────────────

/// Create a kind 10055 PactRequest event.
#[cfg(feature = "nostr-events")]
pub fn create_pact_request_event(
    registry: &NodeRegistry,
    node: NodeId,
    volume: u32,
    min_pacts: u32,
    ttl: u8,
    created_at: u64,
) -> Option<String> {
    let tags = vec![
        Tag::custom(TagKind::custom("volume"), vec![volume.to_string()]),
        Tag::custom(TagKind::custom("min_pacts"), vec![min_pacts.to_string()]),
        Tag::custom(TagKind::custom("ttl"), vec![ttl.to_string()]),
    ];
    create_signed_event(registry, node, 10055, "", tags, created_at)
}

/// Create a kind 10056 PactOffer event.
#[cfg(feature = "nostr-events")]
pub fn create_pact_offer_event(
    registry: &NodeRegistry,
    node: NodeId,
    partner: NodeId,
    volume: u32,
    created_at: u64,
) -> Option<String> {
    let partner_keys = registry.get_keys(partner)?;
    let tags = vec![
        Tag::public_key(partner_keys.public_key()),
        Tag::custom(TagKind::custom("volume"), vec![volume.to_string()]),
    ];
    create_signed_event(registry, node, 10056, "", tags, created_at)
}

/// Create a kind 10053 StoragePact event.
#[cfg(feature = "nostr-events")]
pub fn create_storage_pact_event(
    registry: &NodeRegistry,
    node: NodeId,
    partner: NodeId,
    status: &str,
    volume: u32,
    created_at: u64,
) -> Option<String> {
    let partner_keys = registry.get_keys(partner)?;
    let tags = vec![
        Tag::public_key(partner_keys.public_key()),
        Tag::custom(TagKind::custom("type"), vec!["storage".to_string()]),
        Tag::custom(TagKind::custom("status"), vec![status.to_string()]),
        Tag::custom(TagKind::custom("volume"), vec![volume.to_string()]),
    ];
    create_signed_event(registry, node, 10053, "", tags, created_at)
}

/// Create a kind 10054 Challenge event.
#[cfg(feature = "nostr-events")]
pub fn create_challenge_event(
    registry: &NodeRegistry,
    challenger: NodeId,
    challenged: NodeId,
    nonce: u64,
    created_at: u64,
) -> Option<String> {
    let challenged_keys = registry.get_keys(challenged)?;
    let tags = vec![
        Tag::public_key(challenged_keys.public_key()),
        Tag::custom(TagKind::custom("nonce"), vec![nonce.to_string()]),
    ];
    create_signed_event(registry, challenger, 10054, "", tags, created_at)
}

/// Create a kind 10057 DataRequest event.
#[cfg(feature = "nostr-events")]
pub fn create_data_request_event(
    registry: &NodeRegistry,
    requester: NodeId,
    blinded_filter: &str,
    ttl: u8,
    created_at: u64,
) -> Option<String> {
    let tags = vec![
        Tag::custom(TagKind::custom("filter"), vec![blinded_filter.to_string()]),
        Tag::custom(TagKind::custom("ttl"), vec![ttl.to_string()]),
    ];
    create_signed_event(registry, requester, 10057, "", tags, created_at)
}

// ── Blinded pubkey and challenge hashing ─────────────────────────

/// Compute `H(author_pubkey || date)` for blinded read requests.
#[cfg(feature = "nostr-events")]
pub fn blinded_pubkey_hash(registry: &NodeRegistry, target: NodeId, date: &str) -> Option<String> {
    let keys = registry.get_keys(target)?;
    let mut hasher = Sha256::new();
    hasher.update(keys.public_key().to_bytes());
    hasher.update(date.as_bytes());
    let hash = hasher.finalize();
    Some(hash.iter().map(|b| format!("{:02x}", b)).collect())
}

/// Compute a challenge hash over nostr_json content using SHA-256.
#[cfg(feature = "nostr-events")]
pub fn compute_challenge_hash_nostr(event_jsons: &[&str], nonce: u64) -> u64 {
    let mut hasher = Sha256::new();
    for json in event_jsons {
        hasher.update(json.as_bytes());
    }
    hasher.update(nonce.to_le_bytes());
    let hash = hasher.finalize();
    // Take first 8 bytes as u64
    u64::from_le_bytes(hash[..8].try_into().unwrap())
}
