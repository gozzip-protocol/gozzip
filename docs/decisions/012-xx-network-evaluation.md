# ADR 012: xx Network Protocol Evaluation

**Date:** 2026-03-23
**Status:** Accepted

## Context

xx.network (David Chaum, 2018) provides cMix — a precomputation-based mixnet — along with quantum-resistant cryptography (SIDH+DH hybrid), temporal team messaging, and application-level primitives (channels, E2E DMs, file transfer). Evaluated as a potential source of privacy improvements, post-quantum hardening, and feature-set expansion for gozzip.

Key findings from evaluation:

- **cMix is architecturally incompatible.** cMix requires a fixed set of 350+ dedicated mix nodes running precomputation rounds in lockstep. Gozzip is fully peer-to-peer with no dedicated infrastructure. The core insight — batching messages creates anonymity sets that resist traffic analysis — is adoptable without the infrastructure.
- **SIDH was broken in 2022.** The Castryck-Decru attack recovers private keys for all SIDH parameter sets in hours on a laptop. xx.network's quantum-resistant key exchange is fundamentally compromised. NIST post-quantum standards (ML-KEM, ML-DSA) are the correct path forward.
- **WOTS+ is incompatible with Nostr's identity model.** Winternitz One-Time Signatures are quantum-resistant but single-use by design. Nostr events require persistent signing keys (secp256k1 keypairs sign thousands of events). WOTS+ signatures are also 2-8KB, compared to 64 bytes for Ed25519/secp256k1.

## Decision

### 1. Adopt Gossip Batch-and-Shuffle

Collect outbound gossip messages in configurable time windows (100-200ms default), apply Fisher-Yates shuffle to the batch, then broadcast all messages in randomized order. This captures cMix's core privacy insight — batching creates anonymity sets where any message could have originated from any node in the batch — without requiring dedicated mix infrastructure. The anonymity set size equals the batch size.

### 2. Plan Partial HyParView Rotation

Rotate 1-2 of the ~6 active view peers every 2-5 minutes to bound the duration any single peer can observe a node's traffic patterns. This adapts cMix's temporal team concept (ephemeral routing groups) to gossip topology. Must preserve PlumTree broadcast tree stability — rotate only active view slots, not the eager-push tree edges. Requires an upstream PR or fork of iroh-gossip to expose active view manipulation.

### 3. Plan WoT-Filtered Peer Eligibility

Add a peer acceptance callback at the HyParView membership layer. When HyParView proposes a new peer for the active view, check the candidate's WoT score. Filter by minimum score threshold, then select uniformly among eligible peers. This prevents Sybil identities from entering the active view regardless of how many they create. Requires an iroh-gossip upstream PR to expose the peer acceptance hook.

### 4. Use NIST PQ Standards When Ready

Adopt ML-KEM (FIPS 203) for key encapsulation and ML-DSA (FIPS 204) for digital signatures when audited Rust implementations are available. SLH-DSA (FIPS 205) as a conservative backup — hash-based, no lattice assumptions, but larger signatures. Do NOT adopt xx.network's broken SIDH or limited WOTS+.

### 5. PQ-Ready Type System

Evolve `PubKey` and `Signature` types from fixed-size byte arrays to enums that can represent both classical (Ed25519, secp256k1) and future post-quantum key/signature sizes before v1.0. This avoids a breaking wire format change when PQ algorithms are eventually added.

### 6. NIP-44 + NIP-17 for E2E Encrypted DMs

Implement NIP-44 (versioned encryption using XChaCha20-Poly1305 with conversation keys derived via ECDH) and NIP-17 (gift-wrapped sealed events) for end-to-end encrypted direct messages. Transport these over iroh's direct QUIC streams rather than gossip, since DMs are point-to-point.

### 7. iroh-blobs for Large Content

Integrate iroh-blobs for transferring images, video, and other media too large for gossip messages. Attach NIP-94 compatible metadata (file hash, MIME type, dimensions) so events can reference blob content by hash.

### 8. Structured EventFilter

Replace the current `String` filter field in `RequestData` with a structured `EventFilter` type supporting filtering by kind, author, time range, and tags. This enables richer queries and efficient server-side filtering.

### 9. NIP-05 Identity Verification

Add NIP-05 DNS-based identity verification (`user@domain.com` mapping to pubkey) for human-readable user discovery. Optional — not required for protocol participation.

### 10. NIP-28 Channels + NIP-10 Threading

Implement NIP-28 public channels and NIP-10 event threading over gossip topics. Each channel maps to an iroh-gossip topic. Threading enables reply chains and conversation structure within channels.

## Rejected Alternatives

### Full cMix Integration

cMix requires 350+ dedicated mix nodes running synchronized precomputation rounds. Gozzip has no dedicated infrastructure and targets mobile devices. The operational complexity and resource requirements are incompatible with a fully peer-to-peer architecture.

### SIDH Adoption

Supersingular Isogeny Diffie-Hellman was broken by the Castryck-Decru attack in 2022. All parameter sets (SIKEp434 through SIKEp751) can be recovered in hours on commodity hardware. xx.network's quantum-resistant key exchange is fundamentally compromised.

### WOTS+ for Event Signing

Winternitz One-Time Signatures are quantum-resistant but single-use by design. A Nostr keypair signs thousands of events over its lifetime. WOTS+ would require a new key for every event, destroying the persistent identity model. Signatures are also 2-8KB versus 64 bytes for secp256k1/Ed25519.

### xx.network Centralized User Discovery

xx.network uses centralized UD servers for user lookup. This contradicts gozzip's decentralization goals. NIP-05 provides DNS-based discovery without a single point of control.

### REST-like Message Patterns

xx.network's client API uses REST-style request/response patterns. Gozzip's purpose-built `WireMessage` types (with explicit `RequestData`, `EventEnvelope`, `HaveEvents`, etc.) are more efficient for gossip protocol semantics.

### Full Temporal Teams

cMix rotates the entire routing path every round. In gossip, rotating all active view peers simultaneously destroys the PlumTree broadcast tree, causing message loss during reconvergence. Partial rotation (1-2 peers) preserves tree stability while still limiting observation windows.

### Application-Level Cover Traffic

Generating fake messages to obscure real traffic patterns. Rejected for two reasons: (1) bandwidth is prohibitive on mobile networks, and (2) cover traffic must be cryptographically indistinguishable from real traffic — since gozzip messages carry Nostr signatures, cover messages would need valid signatures on dummy content, which is detectable. QUIC-level padding is the feasible alternative.

## Consequences

**Positive:**
- Gossip privacy improvements (batch-and-shuffle, peer rotation) without infrastructure dependency
- Post-quantum migration path planned with concrete tiers and NIST-standard algorithms
- Feature parity with modern messaging: E2E encrypted DMs, public channels, media transfer
- Structured event filters enable richer queries and more efficient data retrieval
- WoT-filtered peer eligibility strengthens Sybil resistance at the topology level

**Negative:**
- Gossip batch-and-shuffle adds 100-200ms latency to message propagation
- HyParView active view manipulation and peer acceptance callbacks require iroh-gossip fork or upstream PRs
- PQ-ready type system (enum-based keys/signatures) increases wire format complexity
- NIP-44 implementation adds XChaCha20-Poly1305 crypto dependency
- Multiple NIP implementations (05, 10, 17, 28, 44, 94) expand the protocol surface area

**Neutral:**
- xx.network remains inspiration-only (like bitchat in ADR 010) — no code dependency
- FIPS remains dropped per ADR 011 — ML-KEM/ML-DSA are NIST standards but not FIPS-validated yet
- BLE transport still deferred — unrelated to this evaluation
- Post-quantum urgency remains low (priority 3/10) — public gossip data has no harvest-now-decrypt-later threat
