# iroh Network Architecture

## Overview

iroh replaces FIPS as gozzip's transport layer. It provides QUIC-based peer-to-peer connectivity with Ed25519 identity, NAT traversal, and epidemic gossip. By adopting iroh, gozzip gains a production-grade networking stack that handles the complexities of real-world connectivity (firewalls, NATs, mobile networks) while providing cryptographic peer identity at the transport level.

## Connection Model

Each gozzip node is an iroh `Endpoint`. Connections between nodes are QUIC with TLS 1.3, providing mutual authentication via Ed25519 keys baked into the QUIC handshake.

NAT traversal is handled via relay servers with automatic hole-punching upgrade. The lifecycle is:

1. Node starts an iroh `Endpoint`, generating or loading its Ed25519 keypair.
2. The endpoint registers with one or more relay servers.
3. When connecting to a peer, the endpoint first routes through a relay.
4. iroh automatically attempts hole-punching to establish a direct connection.
5. If hole-punching succeeds, traffic migrates to the direct path transparently.

All connections are multiplexed QUIC — a single connection to a peer can carry multiple concurrent streams and topic subscriptions without additional handshakes.

## Gossip Layer

iroh-gossip implements a two-layer protocol:

- **HyParView** (membership): Maintains a partial view of the network. Each node keeps a small *active view* of direct neighbors and a larger *passive view* of known peers. Periodic shuffling ensures connectivity and resilience.
- **PlumTree** (broadcast): Builds a spanning tree over the active view for efficient message dissemination. Falls back to eager push (gossip) when tree links fail, then lazily repairs the tree.

The combination provides topic-based pub/sub with reliable delivery and logarithmic hop counts.

## Topic Design

gozzip uses three categories of gossip topics, each with a deterministic `TopicId`:

### Global Discovery

```
TopicId = sha256("gozzip:discovery:v1")
```

Used for peer discovery, pact discovery, and protocol announcements. All gozzip nodes join this topic. Messages here are protocol-level metadata — never user content.

### Per-Author Topics

```
TopicId = sha256("gozzip:author:{EndpointId}")
```

A follower subscribes to the per-author topic for each author they follow. This is the primary channel for receiving new posts and updates from followed authors. The `EndpointId` is the hex-encoded Ed25519 public key of the author's iroh endpoint.

### Pact Channels

Pact communication uses **direct QUIC bidirectional streams** between pact partners, NOT gossip topics. This is intentional:

- Pact messages are point-to-point by nature (between exactly two parties).
- Gossip would expose pact negotiation to all topic members.
- Direct streams provide lower latency and stronger privacy guarantees.
- Challenge-response protocols require reliable, ordered delivery that direct streams guarantee.

## Message Flow

The message flow follows a hybrid direct + gossip architecture:

1. **Publish**: Author creates and signs a message with their secp256k1 key.
2. **Pact partners**: Message is sent to pact partners via direct QUIC streams. This ensures immediate delivery to storage-committed peers.
3. **Gossip overlay**: Discovery queries and metadata propagate via the gossip overlay.

Important constraints:

- Use `broadcast_neighbors` for bounded fanout, **NOT** `broadcast` (flood). Flooding does not scale and defeats the purpose of HyParView's structured overlay.
- Point-to-point messages (pact negotiation, challenge-response, direct sync) use direct QUIC streams, not gossip.

## Relay Architecture

iroh relays serve as rendezvous points and packet forwarders for nodes that cannot establish direct connections.

**Privacy properties:**
- Relays see only encrypted QUIC packets. They cannot read message content, determine topic membership, or inspect application-layer data.
- Relays *can* see the NodeId (Ed25519 public key) of both communicating parties and packet sizes/timing.

**Operational recommendations:**
- Self-hosted relays are strongly recommended. The default n0 relays concentrate metadata visibility in a single operator.
- The `iroh-relay` crate is open source and straightforward to deploy.
- Multiple relay servers can be configured for redundancy.

## Bootstrap

Node discovery uses a layered approach:

1. **iroh DNS discovery** (primary): iroh's built-in DNS-based node discovery. Nodes publish their addressing information to DNS, and peers resolve it to find connection endpoints.
2. **Nostr relay bootstrap** (secondary): Query Nostr relays for Kind 10070 cross-key binding events to discover iroh NodeIds of known Nostr identities.
3. **Hardcoded bootstrap nodes** (fallback): A small set of well-known gozzip nodes that are always available for initial network entry.

## Scaling

HyParView is designed for scalable partial membership:

- Each node maintains approximately **5 active peers** and **30 passive peers** per topic.
- Active view size is `log(N) + 1` where N is the topic membership.
- The protocol has been tested at **2000 nodes** with stable convergence and message delivery.
- A single QUIC connection multiplexes multiple topics to the same peer, avoiding redundant connections when two nodes share multiple topic subscriptions.
- PlumTree's spanning tree construction minimizes redundant message delivery, achieving near-optimal broadcast with `O(N)` messages for N subscribers.

## Content Addressing (iroh-blobs)

Large content (images, media, long-form text) exceeding gossip message size limits is stored as content-addressed blobs.

### Design

- Events reference large content via `BlobRef` (BLAKE3 hash + size + MIME type) in their `blobs` field.
- Small content (< 32 KiB) is inline in the event.
- Large content is stored as iroh blobs and transferred via iroh-blobs' chunked protocol.
- Storage pact partners replicate both event metadata AND referenced blobs.

### Retrieval

1. Check local blob store.
2. Request from pact partner via direct QUIC stream (iroh-blobs transfer).
3. Request via gossip `BlobRequest` message.
4. Relay fallback.

## Encrypted Direct Messages

Point-to-point DMs use NIP-44 encryption (XChaCha20-Poly1305) with NIP-17 gift wrapping for sender privacy.

- **Transport**: Direct QUIC bidirectional streams (not gossip — DMs are point-to-point).
- **Encryption**: NIP-44 (version 2) with shared secret derived from secp256k1 ECDH.
- **Gift wrapping**: NIP-17 wraps encrypted content in an ephemeral sender envelope, hiding the real sender from intermediate nodes.
- **Nostr compatible**: DMs can be decrypted by any Nostr client using the same secp256k1 keys.

## Channels

Group conversation uses NIP-28 channels over gossip topics:

```
TopicId = sha256("gozzip:channel:{channel_creation_event_id}")
```

- Channel creation, metadata, and messages use dedicated `EventKind` variants.
- NIP-10 `e` tag threading provides structured reply chains.
- No admin keys or centralized moderation — moderation is client-side (muting/hiding).
- WoT filtering applies at the application layer as with all gossip topics.

## Discovery

### NIP-05 Identity Verification

DNS-based identity verification per Nostr NIP-05:

1. User claims identity (e.g., `alice@example.com`).
2. Domain publishes `/.well-known/nostr.json` mapping `alice` → secp256k1 pubkey.
3. Gozzip verifies by fetching the well-known URL and checking the pubkey.
4. Kind 10070 cross-key binding resolves the secp256k1 pubkey to an iroh NodeId.

### Gossip Announcements

Nodes periodically broadcast `Announce` messages on the global discovery topic containing:
- secp256k1 pubkey
- iroh NodeId
- Optional NIP-05 identifier
- Optional display name
