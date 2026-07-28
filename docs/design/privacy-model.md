# Privacy Model

**Status:** Accepted

## The honest metadata story

Gozzip is a censorship-resistant social protocol with practical privacy improvements over mainstream platforms. It is **not** an anonymous communication tool, and this document is deliberate about the difference. The one-line summary: **NIP-44 encrypts content, not the communication graph.** Who talks to whom, who stores whose data, and who reads whose posts are metadata questions, and metadata is where the real exposure lives.

The rest of this document maps what different observers can see, explains why the honest privacy gain over vanilla Nostr comes from *where* metadata flows rather than from hiding anyone, names the surveillance that survives, and describes the mitigations — relay diversity and direct iroh connections — along with an optional gossip extension deferred to future work.

## What NIP-44 protects, and what it does not

NIP-44 (XChaCha20-Poly1305) encrypts the *content* of direct messages and other private payloads. It gives strong content confidentiality: a relay carrying a DM cannot read it. It does nothing for the communication graph. A relay still sees that a message passed between two pubkeys, when, and how large it was.

This is not a Gozzip defect; it is inherent to any system built on published, signed events routed by public key. Every published event is permanently and cryptographically bound to its author's pubkey. That has three consequences worth stating plainly:

- **Non-repudiation.** Unlike Signal's deniable messages, a signed Gozzip event proves authorship to any third party — public posts, reactions, and the outer envelope of DMs alike.
- **Retroactive deanonymization.** If a pseudonymous pubkey is ever tied to a real identity — through an exchange, an operational slip, or legal compulsion — the entire signed history is attributed at once.
- **No expiration.** Signed events persist indefinitely with intact authorship proof wherever they were stored or copied.

Users who need deniability or anonymity should use purpose-built tools (Signal, Tor, Briar). Gozzip complements them; it does not replace them.

## The outbox model fragments metadata

Gozzip inherits Nostr's outbox model: users do not connect to each other, they connect to relays. Alice publishes to her chosen relays (kind 10002); Bob reads her events from one of them; Alice and Bob never learn each other's IP. Each relay sees only the traffic passing through it.

This fragmentation is the primary structural defense. It prevents direct IP exposure between social contacts, and it means no single relay sees any user's full follow list or pact set — each relay sees only a slice. What the outbox model does *not* prevent is the accumulation of partial graphs: a relay operator sees which pubkeys publish to them and which clients fetch from them, and over time those read patterns suggest follow relationships. If most users converge on the same three-to-five popular relays, those operators see a disproportionate share of everything. Relay convergence is a deployment problem, not a protocol one, and it is the lever that turns partial visibility into comprehensive visibility.

## Read privacy: where metadata flows, not requester anonymity

Kind 10057 is a NIP-44-encrypted direct data request sent to a chosen storage peer, addressed over iroh ([ADR 017](../decisions/017-three-tier-retrieval.md)). The request is signed and routed by pubkey; it makes no claim to anonymize the requester. Read privacy is about *where* the metadata flows, not about hiding anyone.

The canonical read-privacy framing is precise:

> **Reader privacy versus vanilla Nostr comes from *where* the metadata flows — to web-of-trust peers rather than arbitrary relay operators — not from requester anonymity.**

When you read a followed author's content, the request goes first to that author's pact partners: people already inside your and the author's social graph. The metadata of "who is reading whom" is exposed to WoT peers who already know the relationship, instead of to a relay operator who is a stranger to it. Nobody is anonymized. The improvement is real but modest, and it is about the *audience* for the metadata, not its absence.

## Challenge-response identifies pact pairs

Pact topology is the most exposed metadata in the storage layer.

Pact partners verify each other with periodic data-availability challenges (kind 10054) — small request/response exchanges at a regular cadence. When those exchanges are mediated by a relay, the regularity is a distinctive signature. **A relay that mediates challenge-response traffic between two specific pubkeys can identify them as a pact pair with near-certainty.** The metronomic pattern of a liveness protocol between exactly two parties over weeks is not something jitter or batching plausibly hides from a persistent observer.

This is the single biggest metadata leak in the storage layer, and it is the reason the two mitigations below matter.

## Surveillance surface by adversary

| Adversary | Sees IP? | Social graph? | Content? | Pact topology? |
|---|---|---|---|---|
| ISP / network tap | Relays; peer IPs if hole-punched | No | No (encrypted) | No (direct-connection patterns are partial evidence) |
| Single relay operator | Connecting clients | Partial (read patterns) | No | Near-certain for pairs whose challenge traffic it mediates |
| Colluding relays (3–5 popular) | Their respective clients | ~80% of the graph | No | Strong (merged timing + mediated challenges) |
| Compromised pact partner | Maybe (if direct sync) | Only their own pact | Their pact's events only | Only their own relationship |
| User's own devices | Yes | Yes | Yes | Yes |

Two structural properties hold across the table. First, **no single point sees the whole picture** — reconstruction requires active collaboration between adversary types. Second, a compromised pact partner learns their *own* relationship and nothing more: they cannot forge events (no signing key), decrypt DMs (separate key), or see the other partners. The colluding-relay row is the real threat model: three to five operators serving a majority of users can reconstruct most of the social graph from merged subscription patterns and identify pact pairs from the challenge traffic they carry.

## Mitigation 1: relay diversity

If a user's relay traffic concentrates on one or two relays, those operators can reconstruct pact topology from challenge timing, sabotage challenge-response by injecting jitter, selectively censor specific event kinds, and merge read patterns into a social graph. Distributing traffic raises the cost and lowers the precision of all four.

Client guidance:

- **Distribute pact-related and challenge traffic across at least three relays**, with no single relay handling more than ~40% of it. Each pact pair picks a relay from the intersection of the partners' relay lists; thin intersections are padded from a default set.
- **Publish events to at least three relays** (already standard under the NIP-65 outbox model), and prompt the user to add alternatives when more than half of their follows share one relay.
- **Fall back across relays for challenges.** If a challenge times out on the primary relay, retry on an alternative before marking it failed, and remember which relay works for that pair.
- **Monitor relay health.** Canary-probe for silently-dropped events, watch per-kind failure rates to catch selective dropping, and track latency to catch a relay injecting delay into challenge-response.

Relay diversity does not defeat a colluding majority — merged partial views still reconstruct most of the graph, and long-term timing analysis still finds probable pairs. It raises the cost. Users facing state-level adversaries should additionally route relay connections through Tor.

## Mitigation 2: direct iroh connections shrink the surface

Direct iroh transport ([ADR 011](../decisions/011-iroh-transport-integration.md)) materially shrinks the metadata surface for pact communication. Under iroh, pact challenge-response and bulk sync run over **direct QUIC bidirectional streams between the two partners**, not relay-mediated NIP-46 blobs. For any pair that successfully hole-punches a direct connection, the relay is removed from the challenge path entirely — the near-certain pact-pair identification described above simply does not happen at any relay, because no relay carries that traffic.

The residual exposures are narrower and honestly stated:

- **iroh relays as fallback.** When hole-punching fails, an iroh relay forwards opaque QUIC packets as a rendezvous point. It sees the NodeId (Ed25519 key) pair and packet sizes/timing, but not content, topic membership, or the secp256k1 social identity. Self-hosted iroh relays are strongly recommended; the default shared relays concentrate this metadata in one operator.
- **IP exposure is the trade-off.** A successful hole-punch reveals each peer's real IP to the other. A pact partner therefore learns the user's IP for their own relationship — but only their own, not the other partners'. Direct connections should be upgraded only for WoT-verified peers; all other traffic stays relay-only, accepting latency in exchange for IP privacy. With N partners on direct connections, N independent parties each learn the user's IP, so in adversarial settings relay-mediated channels remain the safer default.

## DM metadata

NIP-17 gift wrapping hides the real sender from intermediate nodes, but the relay that routes a DM still sees the recipient in the `p` tag (required for routing) and can often resolve the sender's device pubkey to a root identity. The relay therefore learns both ends of every DM exchange, plus timing and frequency, even though NIP-44 keeps the content opaque. Reducing this further (DM relay rotation, batched delivery, poll-based recipient identifiers) is possible but not yet specified; the honest current statement is that Gozzip encrypts DM content but does not hide DM communication graphs.

## Privacy non-goals

Stated plainly, so expectations are accurate. Gozzip does **not** provide: anonymity (every event is bound to its author's pubkey); content deniability (signed events are non-repudiable); read privacy against a motivated adversary (requests are signed and routed by pubkey — the improvement is where metadata flows, not hiding the reader); forward secrecy for public content (public events are permanently attributable); protection against relay collusion (a colluding majority reconstructs most of the graph); or hidden DM communication graphs.

What it does provide: censorship resistance through distributed storage, a key hierarchy that contains device compromise, encrypted DM content, WoT-bounded gossip that limits how far information propagates, and relay fragmentation that raises the cost of comprehensive surveillance.

## Deferred: batch-and-shuffle gossip

Batch-and-shuffle gossip is an optional gossip-privacy extension, deferred to future work and not part of protocol v1. It is recorded here so the intent is not lost.

The core idea, adapted from mixnet analysis, is **batch-and-shuffle forwarding**: instead of forwarding each gossip message immediately, a node buffers outbound messages for a short window (100–200ms), applies a Fisher-Yates shuffle with a cryptographically secure RNG, then broadcasts the batch in randomized order. This breaks the 1:1 timing correlation between an incoming message and its forward; the anonymity set equals the batch size. The cost is latency (100–200ms per hop) and weak anonymity during low-traffic periods. Two companion ideas — partial HyParView view rotation to bound how long any peer observes a node's traffic, and WoT-filtered peer eligibility to keep Sybils out of the active view — both require upstream iroh-gossip API extensions and are likewise deferred. QUIC-level frame padding (normalizing packet sizes to fixed buckets) is available today from iroh's QUIC stack and complements batching by defeating size fingerprinting, though it hides neither timing nor volume.

None of this is required for a conforming v1 client. It is preserved as the roadmap for hardening gossip metadata privacy if and when the network's threat model warrants it.
