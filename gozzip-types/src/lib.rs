//! Shared wire-format types for the Gozzip protocol.
//!
//! These types are serialized with postcard and transmitted between peers
//! over iroh-gossip and direct QUIC streams. They use fixed-size byte
//! arrays for keys and signatures to avoid coupling to any specific
//! crypto library.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// Legacy alias — use CryptoKey for new code.
pub type PubKey = [u8; 32];

/// Legacy alias — use CryptoSignature for new code.
pub type Signature = [u8; 64];

/// A 32-byte hash (SHA-256 / BLAKE3).
pub type Hash = [u8; 32];

/// Cryptographic public key supporting multiple algorithms.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CryptoKey {
    /// Ed25519 or compressed secp256k1 (32 bytes).
    Ed25519([u8; 32]),
    /// Placeholder for future ML-DSA public keys.
    /// ML-DSA-65 public keys are 1952 bytes.
    MlDsa65(Vec<u8>),
}

/// Cryptographic signature supporting multiple algorithms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CryptoSignature {
    /// secp256k1 or Ed25519 signature (64 bytes).
    Classical(#[serde(with = "BigArray")] [u8; 64]),
    /// Placeholder for future ML-DSA signatures.
    /// ML-DSA-65 signatures are 3293 bytes.
    MlDsa65(Vec<u8>),
}

/// Event kinds matching the Nostr event model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventKind {
    Note,
    Reaction,
    Repost,
    Dm,
    LongForm,
    /// NIP-44 encrypted direct message (gift-wrapped).
    EncryptedDm,
    /// NIP-17 gift wrap envelope.
    GiftWrap,
    /// NIP-17 seal (inner encrypted layer).
    Seal,
    /// NIP-28 channel creation.
    ChannelCreate,
    /// NIP-28 channel metadata update.
    ChannelMeta,
    /// NIP-28 channel message.
    ChannelMessage,
}

/// Delivery path classification for metrics and routing decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryPath {
    /// Delivered from local pact partner storage.
    PactPartner,
    /// Delivered from a cached endpoint (known storage peer).
    CachedEndpoint,
    /// Delivered via gossip network propagation.
    Gossip,
    /// Delivered via relay fallback.
    Relay,
}

/// WoT tier classification for trust-weighted routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WotTier {
    /// Mutual follows (highest trust).
    InnerCircle,
    /// One-hop follows + referral-promoted authors.
    Orbit,
    /// Two-hop follows scored by endorsement count.
    Horizon,
    /// Unknown — not in WoT graph.
    Unknown,
}

/// Structured event filter for data requests (Nostr REQ compatible).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventFilter {
    /// Filter by event IDs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<Hash>,
    /// Filter by author public keys.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<PubKey>,
    /// Filter by event kinds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<EventKind>,
    /// Events created after this timestamp (inclusive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    /// Events created before this timestamp (inclusive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<u64>,
    /// Maximum number of events to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Reference to a content blob stored via iroh-blobs.
///
/// Large content (images, media, long-form text) is stored as
/// iroh blobs and referenced by hash in events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobRef {
    /// BLAKE3 hash of the blob content (iroh-blobs content address).
    pub hash: Hash,
    /// Size of the blob in bytes.
    pub size: u64,
    /// MIME type hint (e.g., "image/jpeg", "text/markdown").
    pub mime_type: String,
}

/// NIP-44 encrypted content for DMs and sealed events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedContent {
    /// NIP-44 version byte (currently 2).
    pub version: u8,
    /// Encrypted payload (XChaCha20-Poly1305).
    pub ciphertext: Vec<u8>,
    /// Conversation key nonce (32 bytes).
    pub nonce: [u8; 32],
}

/// A signed event in the Gozzip protocol.
///
/// Events are self-authenticating: the `signature` field contains the
/// author's secp256k1 signature over the event content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// SHA-256 hash of the canonical event content.
    pub id: Hash,
    /// Author's secp256k1 public key (compressed, 32 bytes).
    pub author: PubKey,
    /// Event kind.
    pub kind: EventKind,
    /// Size of the full event content in bytes.
    pub size_bytes: u32,
    /// Per-author monotonic sequence number.
    pub seq: u64,
    /// Hash of the previous event in the author's chain.
    pub prev_hash: Hash,
    /// Unix timestamp (seconds since epoch).
    pub created_at: u64,
    /// secp256k1 signature over (id, author, kind, seq, prev_hash, created_at).
    #[serde(with = "BigArray")]
    pub signature: Signature,
    /// SHA-256 hash of the actual content payload (for content-addressed storage).
    pub content_hash: Hash,
    /// Optional blob references for attached media/files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blobs: Vec<BlobRef>,
}

/// A storage pact between two peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pact {
    /// Public key of the pact initiator.
    pub initiator: PubKey,
    /// Public key of the pact partner.
    pub partner: PubKey,
    /// Agreed storage volume in bytes.
    pub volume_bytes: u64,
    /// Unix timestamp when the pact was formed.
    pub formed_at: u64,
    /// Whether this is a standby pact.
    pub is_standby: bool,
    /// Initiator's signature over the pact terms.
    #[serde(with = "BigArray")]
    pub initiator_sig: Signature,
    /// Partner's countersignature (zeroed until accepted).
    #[serde(with = "BigArray")]
    pub partner_sig: Signature,
}

/// A checkpoint for event reconciliation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Author's public key.
    pub author: PubKey,
    /// Merkle root hash over all events up to `seq`.
    pub merkle_root: Hash,
    /// Highest sequence number covered.
    pub seq: u64,
    /// Unix timestamp of the checkpoint.
    pub created_at: u64,
    /// Author's signature over (merkle_root, seq, created_at).
    #[serde(with = "BigArray")]
    pub signature: Signature,
}

/// Wire messages exchanged between Gozzip peers over iroh-gossip
/// and direct QUIC streams.
///
/// Every message includes the sender's public key and signature
/// to prevent forgery (iroh-gossip's delivered_from is the relay
/// peer, not the original author).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WireMessage {
    /// Publish a new event to pact partners and gossip.
    Publish(Event),

    /// Request data for an author via gossip.
    RequestData {
        from: PubKey,
        request_id: u64,
        ttl: u8,
        /// Structured event filter (replaces single-author PubKey filter).
        filter: EventFilter,
        /// Monotonic nonce for replay protection.
        nonce: u64,
        /// Unix timestamp for time-windowed replay rejection.
        timestamp: u64,
        /// Sender's signature.
        #[serde(with = "BigArray")]
        signature: Signature,
    },

    /// Deliver events in response to a request or pact obligation.
    DeliverEvents {
        from: PubKey,
        events: Vec<Event>,
        path: DeliveryPath,
        request_id: Option<u64>,
        #[serde(with = "BigArray")]
        signature: Signature,
    },

    /// Request to form a storage pact.
    PactRequest {
        from: PubKey,
        volume_bytes: u64,
        as_standby: bool,
        created_at: u64,
        activity_tier: u8,
        #[serde(with = "BigArray")]
        signature: Signature,
    },

    /// Offer pact terms.
    PactOffer {
        pact: Pact,
    },

    /// Accept a pact (includes countersignature).
    PactAccept {
        pact: Pact,
    },

    /// Drop an existing pact.
    PactDrop {
        from: PubKey,
        partner: PubKey,
        #[serde(with = "BigArray")]
        signature: Signature,
    },

    /// Storage challenge (proof-of-storage verification).
    Challenge {
        from: PubKey,
        nonce: u64,
        #[serde(with = "BigArray")]
        signature: Signature,
    },

    /// Response to a storage challenge.
    ChallengeResponse {
        from: PubKey,
        /// SHA-256 proof over stored events + nonce.
        proof: Hash,
        #[serde(with = "BigArray")]
        signature: Signature,
    },

    /// NIP-17 gift-wrapped encrypted DM.
    EncryptedDm {
        /// Ephemeral sender key (disposable, for gift wrapping).
        from: PubKey,
        /// Recipient's public key.
        to: PubKey,
        /// NIP-44 encrypted content containing the sealed event.
        content: EncryptedContent,
        #[serde(with = "BigArray")]
        signature: Signature,
    },

    /// NIP-28 channel message broadcast.
    ChannelBroadcast {
        from: PubKey,
        /// Channel ID (hash of the channel creation event).
        channel_id: Hash,
        /// The channel message event.
        event: Event,
    },

    /// Discovery announcement on the global topic.
    Announce {
        /// Node's secp256k1 public key.
        from: PubKey,
        /// Node's iroh Ed25519 NodeId (32 bytes).
        node_id: [u8; 32],
        /// Optional NIP-05 identifier (e.g., "alice@example.com").
        nip05: Option<String>,
        /// Optional display name.
        display_name: Option<String>,
        /// Unix timestamp.
        timestamp: u64,
        #[serde(with = "BigArray")]
        signature: Signature,
    },

    /// Request blob transfer for content referenced in an event.
    BlobRequest {
        from: PubKey,
        /// The blob to fetch.
        blob: BlobRef,
        /// Request ID for correlation.
        request_id: u64,
        #[serde(with = "BigArray")]
        signature: Signature,
    },
}

/// Protocol version for forward compatibility.
pub const PROTOCOL_VERSION: u8 = 2;

/// Maximum message size for gossip broadcast (64 KiB).
pub const MAX_GOSSIP_MESSAGE_SIZE: usize = 65_536;
