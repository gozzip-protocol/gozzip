# ADR 011: Iroh Transport Integration

**Date:** 2026-03-19
**Status:** Accepted

## Context

Gozzip's simulator exercises the protocol at 5000 nodes but has no real networking. The protocol needs a maintained, production-grade peer-to-peer transport. iroh (v0.97.0) provides exactly that: a QUIC layer with Ed25519 endpoint identity, NAT traversal, and iroh-gossip (epidemic broadcast trees via HyParView + PlumTree) with topic-based pub/sub — a natural fit for gozzip's WoT-filtered gossip model, on a stack we do not have to maintain ourselves.

## Decision

### 1. Approach B: Library + Two Binaries

Extract protocol logic into gozzip-types shared crate. Simulator stays untouched for deterministic validation. New gozzip-node binary implements real networking via iroh. One protocol implementation, two transports.

### 2. iroh as the Transport Layer

Adopt iroh for all peer-to-peer connectivity. Ed25519 (iroh's endpoint identity) is battle-tested (Signal, Tor, SSH, WireGuard), and QUIC gives encrypted, authenticated, migration-capable connections out of the box.

### 3. iroh-gossip for Peer Discovery and Message Propagation

Topic-based gossip maps to gozzip's WoT tiers. Topic-per-author for InnerCircle/Orbit, topic-per-community for Horizon.

### 4. Independent Ed25519 Key

Not derived from Nostr secp256k1 seed. Compartmentalization. Root key compromise doesn't compromise transport.

### 5. Cross-Key Binding Attestation (Kind 10070)

Signed attestation linking Ed25519 and secp256k1 keys. Both keys sign each other. Required before any peer trusts the binding.

### 6. Postcard Wire Serialization

Compact, fast, serde-native. ~40-50 bytes for event envelope.

### 7. All Protocol Messages Signed by secp256k1 Key

iroh-gossip's delivered_from ≠ author. Current simulator trusts from: u32 implicitly — broken in real gossip.

## Rejected Alternatives

### Approach A: New Binary, No Shared Library

Two implementations of same protocol. Maintenance nightmare. Protocol changes must be duplicated.

### Approach C: Replace Simulator Router with iroh

5000 iroh endpoints on one machine infeasible. Loses deterministic simulation. Tests become flaky.

### Derived Ed25519 Key from secp256k1 Root

Expands blast radius of root key compromise. Already identified as critical weakness in review-cryptography.md.

## Consequences

**Positive:**
- E2E encryption for all peer communication (TLS 1.3 via QUIC)
- Mutual authentication at transport layer (Ed25519)
- Forward secrecy via ephemeral key exchange
- Real gossip protocol with formal basis (HyParView + PlumTree papers)
- NAT traversal and relay fallback built-in
- Reduced relay metadata exposure (iroh relays see encrypted QUIC, not event content)
- Simulator preserved for deterministic protocol validation

**Negative:**
- iroh-gossip topics have no authorization — WoT filtering must be reimplemented at application layer
- HyParView's open membership introduces sybil/eclipse attack surfaces not modeled in simulator
- iroh pre-1.0 (API churn — 0.94 renamed NodeId→EndpointId)
- Dual-key identity adds complexity (Ed25519 + secp256k1)
- Default iroh relays concentrate metadata in n0-computer — self-hosted relays needed
- IP address exposure to all direct connection peers (including potential sybils)

**Neutral:**
- BLE transport deferred — iroh's custom transport API is experimental, only Tor works. Design for future addition via QUIC multipath.
- bitchat remains inspiration for BLE mesh (ADR 010) — no code dependency on iroh
