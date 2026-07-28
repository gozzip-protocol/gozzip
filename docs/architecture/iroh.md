# How Gozzip Rides iroh

**Status:** Accepted (see [ADR 011](../decisions/011-iroh-transport-integration.md))

## Overview

[iroh](https://iroh.computer) is Gozzip's transport layer. It provides QUIC-based peer-to-peer connectivity with Ed25519 endpoint identity, NAT traversal, and epidemic gossip. Adopting iroh gives Gozzip a production-grade networking stack that handles the messy realities of connectivity — firewalls, NATs, mobile networks — while providing cryptographic peer identity at the transport level. This document is the single page a newcomer should read to understand how the protocol sits on top of it: the dual-key identity model, the QUIC connection lifecycle, the gossip overlay, and the security work that Gozzip must do at the application layer because iroh does not do it for us.

The one structural thing to hold onto: **iroh moves bytes and authenticates endpoints; Gozzip decides who is trusted.** iroh gossip topics are open, so every trust decision — who may join a conversation, whose messages count, whose IP we expose ourselves to — happens above iroh, in Gozzip's web-of-trust layer.

## Dual-key identity

Each Gozzip node carries two distinct cryptographic identities on two different curves:

- **Ed25519** — the iroh transport identity. Used by the iroh `Endpoint` for QUIC handshakes, peer authentication, and gossip. This is the `NodeId` in iroh terms. It is the node's *network* identity.
- **secp256k1** — the Nostr application identity. Used to sign Nostr events, authenticate application messages, and establish social relationships (follows, pacts, WoT). It is the user's *social* identity.

The two keys are independent: the Ed25519 key is **not** derived from the secp256k1 key or vice versa. This is deliberate compartmentalization. Compromise of the social root key does not compromise the transport layer, and compromise of the transport key does not compromise the Nostr identity; each can be rotated independently and stored in a different backend.

**Key storage.** The Ed25519 key is permanently hot — every new peer connection needs it for the QUIC handshake — so it should live in hardware-backed storage where available (Apple Secure Enclave, Android StrongBox, TPM 2.0), with signing delegated to the secure element. Where hardware backing is unavailable it is encrypted at rest behind device biometrics or a PIN. It is device-specific and MUST be excluded from cloud backup (iCloud Keychain, Google Backup); syncing it to the cloud would defeat device-level compartmentalization.

**Key rotation** is disruptive and should be rare. Changing the Ed25519 key changes the `NodeId`, which loses all HyParView gossip memberships (peers are tracked by NodeId, and a new NodeId is a new peer with no history), requires re-publishing the cross-key binding with fresh signatures, and requires rejoining every topic. iroh has no in-band "NodeId X is now NodeId Y" signal, so rotation is announced out of band through an updated kind-10070 event on the Nostr relay layer, where peers can discover the new NodeId.

## Cross-key binding (kind 10070)

A signed attestation links the two keys into a verifiable pair, published as a Nostr replaceable event, **kind 10070**. Kind 10070 is exclusively this iroh binding.

The binding requires **bidirectional signatures**:

1. The secp256k1 key signs the Ed25519 public key — proving the Nostr identity claims ownership of the iroh node.
2. The Ed25519 key signs the secp256k1 public key — proving the iroh node claims association with the Nostr identity.

Both directions are required, because either alone enables an identity-mismatch attack: with only the secp256k1 signature an attacker could claim someone else's Ed25519 node; with only the Ed25519 signature an attacker could claim someone else's Nostr identity. The kind-10070 event carries both public keys and both signatures, so any verifier can confirm the binding without contacting either key holder. This event is the hinge that lets the WoT (defined over secp256k1 social identities) govern the gossip overlay (defined over Ed25519 NodeIds).

## Connection model

Each Gozzip node is an iroh `Endpoint`. Connections are QUIC with TLS 1.3, mutually authenticated by the Ed25519 keys baked into the handshake via self-signed certificates — there are no anonymous connections at the transport level. A single QUIC connection is multiplexed: it carries multiple concurrent streams and topic subscriptions to the same peer without additional handshakes.

NAT traversal is **relay-assisted hole-punching**:

1. The endpoint starts, loading or generating its Ed25519 keypair, and registers with one or more relay servers.
2. To reach a peer, the endpoint first routes through a relay (a rendezvous point).
3. iroh automatically attempts to hole-punch a direct path.
4. On success, traffic migrates to the direct connection transparently; on failure, it stays relay-forwarded.

TLS 1.3 provides end-to-end encryption and forward secrecy by default: no plaintext is on the wire even through a relay, and compromise of a long-term key does not reveal past session traffic.

## Gossip: HyParView + PlumTree

iroh-gossip is a two-layer epidemic protocol:

- **HyParView (membership)** maintains a partial view of the network — a small *active view* of direct neighbors (about 5 peers) and a larger *passive view* of backup candidates (about 30). Active view size scales as roughly `log(N) + 1`. Periodic shuffling keeps the overlay connected.
- **PlumTree (broadcast)** builds a spanning tree over the active view for efficient dissemination, falling back to eager push when tree links fail and lazily repairing the tree afterward.

Together they give topic-based pub/sub with reliable delivery and logarithmic hop counts, tested to stable convergence at 2,000 nodes. Gossip uses `broadcast_neighbors` for bounded fan-out, never `broadcast` (flood), which does not scale and defeats the structured overlay.

Gozzip uses three topic categories, each with a deterministic `TopicId`:

- **Global discovery** — `sha256("gozzip:discovery:v1")`, joined by all nodes, carrying only protocol metadata (peer/pact discovery, announcements), never user content.
- **Per-author** — `sha256("gozzip:author:{EndpointId}")`, subscribed by followers to receive an author's posts.
- **Channels** — `sha256("gozzip:channel:{creation_event_id}")` for NIP-28 group conversation, threaded with NIP-10 `e` tags, moderated client-side.

**Pact communication does not use gossip.** Pact negotiation, challenge-response, and direct sync run over direct QUIC bidirectional streams between exactly two partners. This is intentional: those exchanges are point-to-point, gossip would expose them to a whole topic, and challenge-response needs the reliable ordered delivery that a direct stream guarantees. Direct messages likewise use direct QUIC streams with NIP-44 encryption and NIP-17 gift wrapping, and remain decryptable by any Nostr client holding the same secp256k1 keys.

## WoT filtering at the application layer

This is the security work iroh does not do. **iroh-gossip topics have no authorization** — any node that knows a `TopicId` can join, observe all messages, inject messages, and influence HyParView membership. Since deterministic TopicIds (like the discovery topic) are trivially derivable by anyone reading this page, Sybil attacks are the primary threat and must be answered above iroh.

**The Sybil/eclipse mitigation** is WoT-filtered peer admission. When iroh-gossip reports a new peer joining the active view (`NeighborUp`), the node:

1. Requests the peer's cross-key binding (kind 10070).
2. Verifies the bidirectional signatures.
3. Checks WoT membership for the resolved secp256k1 identity.
4. Rejects peers that are unknown to the WoT — refusing them entry to the application-layer view.

This directly counters eclipse attacks (flooding a target's small active view with Sybils to cut it off from honest peers): admission is restricted to peers holding valid WoT credentials, which is far harder to obtain than a fresh Ed25519 keypair. Adversarial HyParView view manipulation is a transport-layer attack surface the simulator must cover.

**Message authentication is mandatory and independent of the gossip path.** iroh's `delivered_from` identifies only the neighbor that forwarded a message, not its author, so every Gozzip message is signed by the sender's secp256k1 key (32-byte compressed pubkey + 64-byte Schnorr signature). On receipt every node MUST extract the pubkey, verify the signature over the payload, confirm the pubkey maps to a known NodeId via the kind-10070 binding, and check WoT membership — even for messages from a "trusted" neighbor. Skipping verification is a critical vulnerability.

**Replay protection** is also Gozzip's own job, because iroh dedupes only by content hash. Each sender maintains a strictly increasing per-sender nonce; receivers track the highest nonce seen per sender and reject anything less than or equal to it. A 5-minute timestamp window bounds the replay opportunity and the receiver state required. Both checks apply together: a message needs a fresh nonce *and* a timestamp inside the window.

**Topic privacy**, for the cases that need it, comes from encrypting topic payloads with a symmetric key distributed via NIP-44 DMs to WoT-verified peers, and from using random out-of-band `TopicId` values for private groups so an attacker must learn the TopicId from a participant.

## Relay trust and IP exposure

iroh relays forward opaque QUIC packets. They **cannot** see message content, topic membership, or the secp256k1 social identity. They **can** see the NodeId pair (which endpoints are communicating), packet sizes and timing, and session duration — enough metadata for traffic analysis. The default shared relays concentrate this visibility in a single operator, so **self-hosted relays are strongly recommended** for any deployment where metadata privacy matters; the `iroh-relay` crate is open source and built for easy self-hosting, and multiple relays can be configured for redundancy.

Hole-punching, by design, reveals each peer's real IP to the other, which combined with NodeIds creates a persistent identity-to-location mapping. This includes any Sybil that joined a shared topic and triggered a direct connection. The mitigation is **relay-only mode for non-WoT peers**: upgrade to direct connections only for WoT-verified peers, and keep everything else relay-forwarded, accepting the latency and bandwidth cost in exchange for IP privacy. The broader metadata analysis lives in [privacy-model.md](../design/privacy-model.md).

## Content addressing (iroh-blobs)

Large content — images, media, long-form text — exceeding gossip message limits is stored as content-addressed blobs. Events reference them via a `BlobRef` (BLAKE3 hash + size + MIME type) in their `blobs` field; content under 32 KiB stays inline in the event. Pact partners replicate both event metadata and referenced blobs. Retrieval walks the same tiers as everything else: local blob store, then a direct request to a pact partner (iroh-blobs chunked transfer over a QUIC stream), then relay fallback.

## Discovery

Node discovery is layered: iroh DNS discovery is primary (nodes publish addressing to DNS, peers resolve it); Nostr relay bootstrap is secondary (query relays for kind-10070 bindings to find the NodeIds of known Nostr identities); a small hardcoded set of well-known nodes is the fallback for initial network entry. On top of this, NIP-05 gives DNS-based identity verification (`alice@example.com` → secp256k1 pubkey via `/.well-known/nostr.json`), and the kind-10070 binding then resolves that pubkey to an iroh NodeId. Nodes also periodically broadcast `Announce` messages on the discovery topic carrying their secp256k1 pubkey, NodeId, and optional NIP-05 identifier.

## A note on "FIPS" — a naming collision

**"FIPS" in the original whitepaper means the *Free Internetworking Peering System* — a bespoke mesh transport — not the NIST FIPS 140 cryptographic-validation standard.** The two share three letters and nothing else.

The Free Internetworking Peering System is superseded by iroh because it was pre-production. iroh is a maintained, production-grade QUIC stack that delivers equivalent transport independence without Gozzip having to build and operate a mesh from scratch ([ADR 011](../decisions/011-iroh-transport-integration.md)). This is a transport-maturity decision.

Separately, NIST FIPS 140 cryptographic validation is a non-goal: Gozzip deliberately uses the best cryptography for its threat model (Ed25519 is not FIPS-approved, secp256k1 is not a FIPS curve, `rustls` is not FIPS-validated) rather than conforming to a compliance framework that would require different primitives. The transport-maturity decision and the crypto-policy stance are independent.

## Post-quantum readiness

The wire-format types (`CryptoKey`, `CryptoSignature`) are enums that already accommodate post-quantum algorithm variants alongside the classical fixed-size ones, so the migration path is staged and mostly transport-driven: PQ-hybrid QUIC arrives when iroh's `rustls` upgrades (no Gozzip code change), optional ML-DSA signatures follow behind a feature flag once audited, and hybrid ML-KEM key exchange arrives with end-to-end DM work. Full detail is deferred to the post-quantum roadmap in `docs/future/`.
