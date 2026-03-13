# Interplanetary Protocol Properties

An honest assessment of which Gozzip protocol properties are genuinely useful for interplanetary communication, which need adaptation, and which claims are indefensible.

## Context

The protocol was not designed for interplanetary use. However, several architectural choices — made for terrestrial reasons — happen to align with the requirements of delay-tolerant networking (DTN). This document separates the real from the aspirational.

## Physical Constraints

| Route | One-way latency | Round-trip | Conjunction blackout |
|---|---|---|---|
| Earth–Mars (closest) | 3 min | 6 min | ~2 weeks every 26 months |
| Earth–Mars (average) | 12.5 min | 25 min | — |
| Earth–Mars (farthest) | 22 min | 44 min | — |
| Earth–Moon | 1.3 sec | 2.6 sec | — |
| Earth–LEO station | <100 ms | <200 ms | — |

Solar conjunction creates a 2-week blackout where Earth and Mars cannot communicate at all. Any protocol that requires synchronous interaction fails during conjunction.

## What Genuinely Works

### Self-Authenticating Events

Every Gozzip event is signed by the author's secp256k1 key. Verification requires only the public key — no server, no certificate authority, no online check. An event received via data mule from Mars is exactly as verifiable as one received from a local relay. This is the single most important interplanetary property.

### Store-and-Forward Architecture

The protocol already queues events when no transport is available (up to 1,000 events or 50 MB) and drains when any transport becomes available. This is precisely the store-and-forward model that DTN requires. No protocol changes needed.

### Checkpoint Reconciliation

Checkpoints (kind 10051) with Merkle roots enable efficient state comparison between nodes that have been partitioned for extended periods. After a 2-week conjunction blackout, two nodes can compare checkpoint Merkle roots and exchange only the delta — exactly the reconciliation pattern needed for high-latency links.

### Partition Handling

The protocol explicitly handles network partitions: suspending reliability scoring, not initiating pact replacement, and reconciling after the partition heals. A conjunction blackout is a partition — the protocol already handles it correctly. The 3-consecutive-successful-challenges restoration rule works naturally after conjunction ends.

### WoT 2-Hop Boundary as Planetary Containment

A Mars colony of ~2,000 people forms a dense, self-contained WoT graph. The 2-hop gossip boundary naturally contains most Mars gossip within Mars — it doesn't leak across the interplanetary link unless someone on Mars follows someone on Earth (creating a cross-planetary WoT edge). This is a desirable property: it means local gossip stays local without any explicit planetary routing rules.

### Cascading Read-Caches as Data Mules

When a ship arrives at Mars carrying cached Earth events, the crew's devices act as read-caches. Mars residents who query for Earth content get served from these caches — exactly the cascading replication mechanism described in the retrieval protocol. No protocol changes needed. The ship doesn't need to be a special node type; its crew are just users whose read-caches happen to contain Earth content.

## What Needs Adaptation

### DTN Transport Adapter (HIGH priority)

FIPS provides transport abstraction but does not include a DTN-specific transport. For interplanetary operation, the protocol needs a FIPS transport adapter that speaks **Bundle Protocol (RFC 9171)** — the standard DTN protocol used by space agencies. This adapter would:

- Encapsulate Gozzip events as DTN bundles
- Handle custody transfer (the bundle layer takes responsibility for delivery)
- Support scheduled contacts (communication windows)
- Integrate with existing space network infrastructure (DSN, relay satellites)

This is the single highest-priority extension for interplanetary use.

### Latency-Adapted Timeouts (MEDIUM priority)

Several protocol timeouts assume terrestrial latency:

| Parameter | Current value | Interplanetary adaptation |
|---|---|---|
| Gossip TTL timeout | 30s per hop | Must scale to minutes/hours for cross-planetary hops |
| Challenge response window (hash) | 1–24h | Adequate for Mars (44 min RTT fits within 1h window) |
| Challenge response window (serve) | 500ms | Not applicable cross-planet — hash-only challenges |
| Relay fallback timeout | 30s | Must scale for cross-planetary relay |

Most timeouts already accommodate the delays. The main issue is gossip TTL timeout for cross-planetary requests.

### Small-Community Pact Scaling (MEDIUM priority)

A Mars colony of 2,000 people has far fewer potential pact partners than a terrestrial network. The equilibrium-seeking formation model handles this naturally — a small community reaches comfort at fewer pacts because the available partners have higher uptime overlap. However, two adjustments may be needed:

- Lower PACT_FLOOR for communities below 5,000 (12 may be too many when the entire community is 2,000)
- Accept higher ε for small communities (0.01 instead of 0.001 — still 99% availability, just not 99.9%)

### Conjunction-Aware Pact Suspension (LOW priority)

Cross-planetary pacts (if they existed) would need conjunction-aware suspension. But as argued below, cross-planetary pacts should NOT form — making this a non-issue.

## What Does NOT Work

### Cross-Planetary Pacts

Pacts between Earth and Mars nodes should **not** form. The reasons are structural:

1. **Conjunction kills reliability scores.** A 2-week blackout every 26 months means challenge-response fails for 2 continuous weeks. Even with partition handling, the reliability score takes a sustained hit that normal terrestrial pacts don't face.

2. **Challenge-response is meaningless at 44-minute RTT.** Hash challenges prove possession, but the 22-minute one-way latency makes serve challenges impossible. Even hash challenges are awkward — the nonce must travel to Mars, the partner computes the hash, and the response travels back. A 44-minute round trip for a single challenge.

3. **Volume balancing across planets is pointless.** The entire premise of pact reciprocity — "I store your data, you store mine" — breaks when the two parties are 12.5 light-minutes apart and the storage is for disaster recovery that can only be accessed during non-conjunction periods.

4. **The pact adds no availability.** During conjunction, the Earth pact partner is unreachable. During normal operation, Mars residents query Mars pact partners first (Tier 1–3 in the retrieval cascade). The Earth pact partner is strictly Tier 4 fallback with 25-minute latency — worse than a Mars relay.

**The right model:** Each planet forms its own pact mesh. Cross-planetary data exchange happens via data mules (ships, scheduled transmissions) and cascading read-caches, not bilateral pacts.

### Instant Cross-Planetary Retrieval

The gossip retrieval cascade (Tier 3, TTL=3) cannot provide instant delivery across planets. A gossip hop from Earth to Mars takes 3–22 minutes, not 80ms. The protocol's latency guarantees apply within a planetary community, not across them.

### Trust-Weighted Cross-Planetary Gossip Routing

WoT-weighted gossip forwarding works within a planet. Across planets, the gossip boundary naturally separates communities. Cross-planetary gossip requests would need explicit bridge nodes — users who follow people on both planets — and these requests would have minutes-to-hours latency. This works but should not be presented as "gossip" in the colloquial sense of rapid spread.

## Attack Surface: WoT Bridge Compromise

The most dangerous interplanetary attack vector. Cross-planetary social connections flow through **bridge nodes** — users who follow people on both Earth and Mars. In a colony of 2,000 with 5 billion on Earth, there might be only 50–100 active bridge nodes.

**Attack:** Compromise or impersonate bridge nodes to control the information flow between planets. A compromised bridge can:
- Selectively censor Earth content reaching Mars (or vice versa)
- Inject fabricated events attributed to Earth authors
- Delay forwarding to create information asymmetry

**Mitigation:**
- Self-authenticating events prevent fabrication (signatures verify regardless of source)
- Multiple independent bridges provide redundant paths (if one censors, others don't)
- **Bridge diversity monitoring** — clients should track how many independent bridges they receive cross-planetary content from. If content from a given Earth author arrives via only 1 bridge, that's a flag.
- **Cross-bridge checkpoint verification** — compare checkpoint data received via different bridges to detect selective omission

**Residual risk:** Delay attacks (slowing forwarding without dropping) are hard to detect with high latency. Censorship is detectable only when comparing across multiple bridges, which requires bridge diversity in the first place. A colony with very few bridge nodes is structurally vulnerable.

## Other Attack Vectors

| Attack | Severity | Protocol handles? |
|---|---|---|
| Mars Flood (spam from Mars accounts) | Medium | Yes — WoT boundary contains it. Mars spam stays on Mars unless bridge nodes forward it. |
| Stale Pact (Earth partner goes stale during conjunction) | Medium | Partially — partition handling suspends scoring, but extended offline detection needs tuning for conjunction timescales |
| Timeline Manipulation (backdate events during blackout) | Low | Yes — bounded timestamps, hash chains, and checkpoint reconciliation detect it |
| Partition Extension (attacker prolongs perceived conjunction) | Low | Yes — partition detection uses relay connectivity as ground truth |
| Data Mule Poisoning (corrupt data on ships) | Medium | Mostly — self-authenticating events reject forgeries. Hash verification catches corruption. But a mule can selectively omit events (censorship, not corruption). |

## Honest Assessment

**What Gozzip is:** A protocol whose architectural foundations — self-authenticating events, store-and-forward, partition handling, checkpoint reconciliation, WoT-bounded gossip — are compatible with interplanetary requirements. These properties were chosen for terrestrial resilience but happen to satisfy DTN constraints.

**What Gozzip is not:** An interplanetary protocol. It doesn't speak Bundle Protocol, doesn't handle scheduled contacts, doesn't have planetary routing, and doesn't address the specific topology of space networks.

**The defensible claim:** Gozzip is a DTN-compatible social protocol. With a FIPS transport adapter for Bundle Protocol (RFC 9171), the same events, pacts, and gossip mechanisms that work terrestrially can operate over interplanetary links. The protocol's tolerance for partition, latency, and asynchronous operation is not a marketing claim — it's a structural property of the architecture.

**What's needed for real interplanetary use:**

| Extension | Priority | Effort |
|---|---|---|
| DTN transport adapter (Bundle Protocol) | HIGH | Significant — new FIPS transport module |
| Bridge diversity monitoring | HIGH | Medium — client-side logic |
| Latency-adapted timeouts | MEDIUM | Low — parameterize existing timeouts |
| Small-community pact scaling | MEDIUM | Low — adjust FLOOR and ε |
| Conjunction-aware suspension | LOW | Already handled by partition handling |

## Design Decision

The protocol paper should mention interplanetary compatibility as a **structural property that follows from architectural choices** — not as a feature. The FIPS section already describes transport independence; interplanetary compatibility is a consequence of that independence plus self-authenticating events plus partition handling.

The protocol should NOT include interplanetary-specific mechanisms in the core spec. These belong in an extension document if/when real interplanetary deployment is pursued. Baking planetary routing into the core protocol would be premature optimization for a scenario that may never materialize.
