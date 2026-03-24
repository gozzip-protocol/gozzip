# Gossip Privacy Enhancements

Adopted from cMix analysis (ADR 012). These techniques capture the core privacy insights of mixnet design without requiring dedicated mix node infrastructure.

## Batch-and-Shuffle Forwarding

### How It Works

Instead of forwarding gossip messages immediately upon receipt, nodes collect outbound messages into time-windowed batches:

1. **Collect**: Buffer all outbound gossip messages for a configurable window (100-200ms default).
2. **Shuffle**: At window expiry, apply Fisher-Yates shuffle to randomize message order within the batch.
3. **Broadcast**: Send all messages in the shuffled order to gossip peers.

### Why It Matters

Without batching, a network observer correlating ingress and egress timing at a node can trivially determine which incoming message triggered which outgoing forward. Batching breaks this 1:1 timing correlation. The **anonymity set** equals the batch size — if a node forwards 8 messages in one batch, an observer cannot determine which of the 8 was authored locally versus forwarded.

### Configuration

| Parameter | Default | Range | Notes |
|---|---|---|---|
| `batch_window_ms` | 150 | 100-200 | Lower = less latency, smaller anonymity set |
| `min_batch_size` | 1 | 1-10 | Force flush even if window hasn't expired |
| `max_batch_size` | 64 | 16-256 | Flush early if batch exceeds this |

### Trade-offs

- **Latency**: Adds 100-200ms to every hop. For a 4-hop PlumTree path, worst case adds ~800ms total propagation delay. Acceptable for a messaging protocol (not a trading system).
- **Anonymity vs. throughput**: Low-traffic periods produce small batches with weaker anonymity. High-traffic periods produce large batches with strong anonymity. This is inherent to any batching scheme.
- **Implementation**: Requires a per-peer outbound queue with a timer. Straightforward with tokio's `interval` and `select!`.

### Fisher-Yates Shuffle

The shuffle MUST use a cryptographically secure random source (`rand::rngs::OsRng` or `ChaCha20Rng`). A predictable shuffle defeats the purpose. Fisher-Yates runs in O(n) time and O(1) extra space — negligible overhead for batch sizes under 256.

## Partial HyParView Rotation

### Concept

HyParView maintains an **active view** of ~6 peers for direct communication and a larger **passive view** (~30 peers) as backup candidates. In the default protocol, peers are only replaced on failure or explicit disconnect. This means the same peers observe a node's traffic patterns indefinitely.

Partial rotation introduces deliberate churn: every 2-5 minutes, disconnect 1-2 active view peers and promote replacements from the passive view.

### Why Partial, Not Full

Full rotation (replacing all active view peers simultaneously) destroys the PlumTree broadcast tree. PlumTree builds an overlay spanning tree via eager-push and lazy-push links. If all of a node's tree neighbors change at once, the tree must fully reconverge — causing message loss, duplicate deliveries, and latency spikes.

Rotating 1-2 of 6 peers preserves the majority of tree edges. PlumTree reconverges the 1-2 affected links within seconds via its lazy-push repair mechanism.

### Rotation Strategy

1. Select 1-2 active view peers at random (uniform, CSPRNG).
2. Send a DISCONNECT message to selected peers.
3. Promote peers from passive view using standard HyParView JOIN mechanism.
4. If the disconnected peer was an eager-push tree neighbor, PlumTree's PRUNE/GRAFT cycle will repair the tree automatically.

### Timing Parameters

| Parameter | Default | Range | Notes |
|---|---|---|---|
| `rotation_interval_s` | 180 | 120-300 | How often to rotate |
| `rotation_count` | 1 | 1-2 | How many peers to rotate per interval |

### Upstream Dependency

iroh-gossip does not currently expose active view manipulation. This requires either:
- An upstream PR adding `rotate_active_peer()` or `disconnect_peer()` to the `GossipTopic` API.
- A fork of iroh-gossip with the necessary hooks.

The upstream PR path is preferred to avoid fork maintenance burden.

## WoT-Filtered Peer Eligibility

### Problem

HyParView accepts any peer that sends a valid JOIN request. In a Sybil attack, an adversary creates thousands of identities and floods JOIN requests. Eventually, Sybil nodes dominate the active view, enabling traffic analysis or eclipse attacks.

### Solution

Add a **peer acceptance callback** at the HyParView membership layer. When HyParView proposes adding a new peer to the active view:

1. Look up the candidate's WoT score (their Ed25519 `EndpointId` mapped to a Nostr pubkey via the Kind 10070 binding attestation from ADR 011).
2. If the score is below a minimum threshold, reject the candidate.
3. Among eligible candidates, select uniformly at random (do NOT prefer higher scores — that creates a preferential attachment topology vulnerable to targeted attacks on high-WoT nodes).

### Callback Interface

```rust
/// Called by HyParView when considering a peer for the active view.
/// Return true to allow, false to reject.
type PeerFilter = Box<dyn Fn(EndpointId) -> bool + Send + Sync>;
```

### WoT Score Thresholds

| Context | Minimum Score | Rationale |
|---|---|---|
| Active view | > 0.0 | Must have at least one trust path |
| Passive view | any | Allow discovery of new peers |
| Tree neighbor | > 0.0 | Same as active view (subset) |

### Upstream Dependency

Like partial rotation, this requires an iroh-gossip API extension — specifically, a configurable peer acceptance hook in the HyParView implementation. This can be combined with the rotation PR into a single "HyParView extensibility" contribution.

## Cover Traffic Considerations

### Why Application-Level Cover Traffic Was Rejected

Cover traffic (generating fake messages to obscure real traffic patterns) was evaluated and rejected for two reasons:

1. **Bandwidth**: Mobile devices on metered connections cannot sustain constant dummy message generation. Even modest cover traffic (1 msg/sec) at ~500 bytes/msg = 43 MB/day per peer — unacceptable on cellular networks.

2. **Signature distinguishability**: Gozzip messages carry Nostr signatures over real event content. Cover messages would need to be cryptographically indistinguishable from real messages. This means either:
   - Signing dummy events with real keys (pollutes the event store, detectable by content analysis).
   - Using fake signatures (trivially distinguishable — invalid signature = known cover traffic).

   Neither option provides meaningful privacy.

### QUIC-Level Padding: The Feasible Alternative

QUIC supports frame-level padding (RFC 9000, Section 19.1). Padding at the transport layer:

- Is invisible to application-layer analysis (all QUIC frames are encrypted).
- Does not require valid Nostr signatures.
- Can normalize packet sizes to fixed buckets (e.g., 512, 1024, 2048 bytes) to prevent message-size fingerprinting.
- Is supported by iroh's QUIC implementation (quinn).

This does NOT hide traffic timing or volume — only message sizes. It complements batch-and-shuffle (which hides timing) but does not replace a mixnet for full traffic analysis resistance.

## Comparison: What Gozzip Adopts vs. Rejects from cMix

| cMix Feature | Gozzip Adoption | Status | Rationale |
|---|---|---|---|
| Batch-and-shuffle | **Adopted** | Implementable now | Core privacy insight, no infrastructure needed |
| Temporal teams (routing group rotation) | **Adapted** as partial HyParView rotation | Requires iroh-gossip PR | Preserves tree stability, bounds observation window |
| Fixed mix node infrastructure (350+ nodes) | **Rejected** | N/A | Incompatible with P2P architecture |
| Precomputation rounds | **Rejected** | N/A | Requires synchronized dedicated nodes |
| SIDH quantum-resistant key exchange | **Rejected** | N/A | Broken (Castryck-Decru, 2022) |
| WOTS+ one-time signatures | **Rejected** | N/A | Incompatible with Nostr persistent-key model |
| Cover traffic (dummy messages) | **Rejected** (app-level) | N/A | Bandwidth + signature distinguishability |
| Packet size normalization | **Adopted** via QUIC padding | Implementable now | Prevents size fingerprinting |
| Anonymity sets via batching | **Adopted** | Implementable now | Anonymity set = batch size |
| Centralized user discovery | **Rejected** | N/A | Contradicts decentralization; use NIP-05 instead |
