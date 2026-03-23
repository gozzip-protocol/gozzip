# iroh Security Model

## Threat Model Changes

### What's Gained

- **End-to-end encryption**: All peer communication is encrypted via QUIC + TLS 1.3. No plaintext on the wire, even through relays.
- **Mutual authentication**: Every QUIC handshake authenticates both peers via Ed25519 certificates. No anonymous connections.
- **Forward secrecy**: TLS 1.3 provides forward secrecy by default. Compromise of a long-term key does not reveal past session traffic.
- **Formal gossip protocol**: HyParView + PlumTree is a well-studied, peer-reviewed protocol with known convergence properties, unlike ad-hoc gossip designs.

### What's Lost

- **WoT-bounded gossip membership**: iroh gossip topics are open. Any node that knows a `TopicId` can join and participate. There is no built-in authorization gate. Previously, WoT membership could bound who participates in gossip; this must now be enforced at the application layer.
- **Controlled peer set**: HyParView dynamically selects active and passive view members. The application does not have full control over which peers are in its gossip neighborhood. This is by design for resilience, but it means untrusted peers may appear in the active view.

## Sybil Attacks

iroh-gossip topics have **NO authorization**. Any node can join any topic if it knows the `TopicId`. This makes sybil attacks the primary threat.

An attacker can create many iroh nodes (cheap — just generate Ed25519 keys) and join a target topic. From there, sybil nodes can:

- Observe all messages on the topic.
- Inject spurious messages.
- Manipulate HyParView membership views.

**Mitigation: WoT filtering at the application layer.**

When iroh-gossip reports a `NeighborUp` event (a new peer joins the active view):

1. Request the peer's cross-key binding (Kind 10070).
2. Verify bidirectional signatures.
3. Check WoT membership for the secp256k1 identity.
4. **Reject unknown peers** — refuse to accept them into the application-layer view.

**Note:** HyParView active/passive view manipulation is a NEW attack surface that was not modeled in the existing gozzip simulator. The simulator must be extended to account for adversarial HyParView behavior.

## Eclipse Attacks

An eclipse attack occurs when an attacker floods a target's HyParView active view with sybil nodes. If all active view slots are occupied by attacker-controlled nodes, the target is eclipsed: it only receives gossip from the attacker and cannot communicate with honest peers.

HyParView's active view is small (~5 peers), making it particularly susceptible if peer admission is unfiltered.

**Mitigation: WoT-filtered peer admission.** Reject any peer from active and passive views that does not pass WoT verification. This reduces the attack surface to only those sybils that can somehow obtain valid WoT credentials, which is substantially harder than generating Ed25519 keys.

## Topic Privacy

Anyone who knows a `TopicId` can join the corresponding topic and read all messages. `TopicId` values derived from deterministic strings (like `sha256("gozzip:discovery:v1")`) are trivially discoverable by anyone who reads this documentation.

**Mitigations:**

- **Topic content encryption**: Encrypt topic payloads with a symmetric key distributed via NIP-44 DMs to WoT-verified peers. Unauthenticated joiners see only ciphertext.
- **Random TopicIds shared out-of-band**: For private groups, generate random `TopicId` values and share them only with intended participants via encrypted channels. This limits discoverability — an attacker must learn the `TopicId` from a participant.

## Transport Security

- **Protocol**: QUIC + TLS 1.3 via `rustls`.
- **Certificates**: Self-signed Ed25519 certificates. Each node's certificate contains its Ed25519 public key (the `NodeId`).
- **Encryption**: End-to-end. All data between two peers is encrypted, whether routed through a relay or sent via direct connection.
- **Relays see encrypted packets only.** A relay forwards opaque QUIC packets without the ability to inspect their contents.

## Relay Trust

Default n0 relays concentrate metadata visibility in a single operator. While relays cannot read message content, they have significant metadata access:

- **NodeId pairs**: Which peers are communicating.
- **Packet sizes and timing**: Traffic analysis can reveal communication patterns.
- **Connection duration**: How long peers maintain sessions.

**Relays cannot see:**
- Message content (encrypted).
- Topic membership (topic IDs are inside encrypted QUIC streams).
- Application-layer identity (secp256k1 keys are not exposed to relays).

**Recommendation:** Self-hosted relays are strongly recommended for any deployment where metadata privacy matters. The `iroh-relay` crate is open source and designed for easy self-hosting.

## IP Exposure

QUIC hole-punching, by design, reveals the real IP address of both peers to each other. In the context of gozzip:

- After a successful hole-punch, the peer knows your IP address.
- This includes potential sybil nodes that have joined a shared topic and triggered a direct connection.
- IP addresses combined with NodeIds create a persistent mapping of identity to network location.

**Mitigation: Relay-only mode for non-WoT peers.** Only upgrade to direct (hole-punched) connections for peers that have been verified through the WoT. Keep all other connections relay-only, accepting the latency and bandwidth cost in exchange for IP privacy.

## FIPS

FIPS compliance is **explicitly NOT a goal**. The cryptographic primitives used by gozzip and iroh are fundamentally incompatible with FIPS 140:

- **Ed25519**: Not a FIPS-approved algorithm.
- **secp256k1**: Not a FIPS-approved curve (FIPS specifies P-256, P-384, P-521).
- **iroh's TLS 1.3 implementation** (`rustls`): Not FIPS-validated.

No coherent FIPS posture is achievable without replacing the entire cryptographic foundation of both iroh and Nostr. This is not a design deficiency — it is a conscious choice to use the best available cryptography for the threat model rather than conforming to a compliance framework that would require weaker or less suitable primitives.

## Gossip Privacy Enhancements

Following evaluation of xx.network's cMix mixnet protocol (ADR 012), several gossip-native privacy enhancements were identified. These capture cMix's core insights without requiring dedicated mix node infrastructure.

### Batch-and-Shuffle Forwarding

Outbound gossip messages are collected in a configurable time window (default 150ms), shuffled via Fisher-Yates, then broadcast. This breaks temporal correlation between message receipt and forwarding.

- **Anonymity set**: Equal to the batch size (typically 5-15 messages at moderate traffic).
- **Latency cost**: Added delay equals the batch window (100-200ms).
- **Implementation**: Application-layer wrapper around iroh-gossip broadcast. No iroh-gossip fork required.
- **Threat model**: Defeats naive timing analysis by a single observer. Does not defeat a global passive adversary monitoring all nodes simultaneously.

### Partial HyParView Rotation (Planned)

Rotate 1-2 of ~6 active view peers every 2-5 minutes. This bounds persistent traffic observation — an adversary who compromises one neighbor can only observe traffic for one rotation period instead of indefinitely.

- **Requires**: New `Gossip::rotate_peer()` API in iroh-gossip (upstream PR or soft fork).
- **Preserves**: PlumTree broadcast tree stability (only partial rotation, not full).
- **Anti-Sybil**: Forces Sybil nodes to re-establish position each epoch. Persistence probability drops to `(sybil_fraction)^K` over K epochs.

### WoT-Filtered Peer Eligibility (Planned)

Add a peer acceptance callback to HyParView that filters by WoT score before allowing peers into the active view. This prevents Sybil nodes from entering the gossip neighborhood at all, rather than filtering at the message level after they're already connected.

- **Requires**: New `accept_peer` callback trait on `Gossip::builder()` in iroh-gossip.
- **Design**: WoT scores filter the random selection pool, then HyParView selects uniformly from eligible peers. This preserves the privacy of random selection while excluding Sybils.

### Post-Quantum Readiness (Planned)

Wire format types (`CryptoKey`, `CryptoSignature`) support both classical and post-quantum algorithm variants. Migration path:

1. **Tier 0 (current)**: PQ-ready type enums alongside classical fixed-size aliases.
2. **Tier 1 (when iroh upgrades)**: PQ-hybrid QUIC transport via rustls-post-quantum. Zero code changes.
3. **Tier 2 (when audited)**: Optional ML-DSA signatures behind feature flag.
4. **Tier 3 (with E2E DMs)**: Hybrid ML-KEM + X25519 key exchange.

See `docs/design/post-quantum-roadmap.md` for full details.

## Priority Mitigations

### P1 — Critical (implement before any public deployment)

| Mitigation | Purpose |
|---|---|
| Cross-key binding (Kind 10070) | Links iroh and Nostr identities; prevents identity mismatch attacks |
| Message signing (secp256k1) | Authenticates message authorship independent of gossip relay path |
| WoT-filtered HyParView | Prevents sybils from occupying active view slots; blocks eclipse attacks |
| Self-hosted relays | Eliminates centralized metadata collection by default relay operator |

### P2 — Important (implement before scaling beyond trusted users)

| Mitigation | Purpose |
|---|---|
| Topic content encryption | Prevents passive observers from reading topic messages |
| Replay protection (nonces + timestamps) | Blocks message replay attacks at the application layer |
| Direct connection filtering | Limits IP exposure to WoT-verified peers only |
| Hardware key storage (Secure Enclave/TEE) | Protects Ed25519 key from extraction on compromised devices |

### P3 — Hardening (implement as resources allow)

| Mitigation | Purpose |
|---|---|
| TopicId from shared secrets | Makes topic discovery require out-of-band knowledge |
| Merkle proof challenges | Proves data possession without full transfer; verifies pact compliance |
| Key rotation protocol | Enables Ed25519 key changes with minimal disruption to gossip membership |
| Traffic padding | Mitigates traffic analysis at the relay and direct connection level |
| Gossip batch-and-shuffle | Breaks temporal correlation in message forwarding (implemented) |
| Partial HyParView rotation | Bounds persistent traffic observation (planned, needs iroh-gossip PR) |
| WoT peer eligibility filtering | Prevents Sybils from entering active view (planned, needs iroh-gossip PR) |
