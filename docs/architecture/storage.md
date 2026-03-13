# Storage

Who stores what in the Gozzip network. Storage is not centralized — users are each other's infrastructure through reciprocal storage pacts.

## Storage Model

Three tiers of storage, tried in priority order:

| Tier | Who (Persona) | What they store | Availability | Expected Uptime |
|------|---------------|----------------|-------------|-----------------|
| Device | User's own devices | All own events, full history | Only when online | — |
| Storage peers (Full) | ~20 WoT peers running full nodes (*Keepers*) | Complete event history | Distributed — always-on | 95% |
| Storage peers (Light) | ~20 WoT peers running light nodes (*Witnesses*) | Events since last checkpoint (~monthly window) | Distributed — when online | 60% (extension/web) |
| Relays | Third-party infrastructure (*Heralds*) | Whatever their retention policy allows | Optional fallback | — |

## Storage Pacts

Every user commits to storing recent data for ~20 volume-matched peers in their web of trust. The commitment is reciprocal. See [ADR 005](../decisions/005-storage-pact-layer.md).

Reciprocal pacts require WoT membership. Two exceptions: bootstrap pacts (triggered by follow) and guardian pacts (volunteered by an established user). See [Glossary](../glossary.md).

### How pacts form

1. User broadcasts kind 10055 (storage pact request) with their data volume
2. WoT peers with similar volume respond with kind 10056 offers
3. User selects partners — both exchange private kind 10053 pact events
4. Both begin storing each other's events from the current checkpoint forward

### Volume balancing

Peers are matched by data volume (+/- 30% tolerance) so the storage commitment is symmetric. If a partner's volume drifts beyond tolerance:

1. Protocol flags the pairing as unbalanced
2. Client waits `random(0, 48h)` before broadcasting (jittered delay prevents renegotiation storms when many users detect the same peer failure simultaneously). During the delay, standby pacts provide immediate failover. See [ADR 008](../decisions/008-protocol-hardening.md).
3. User broadcasts kind 10055 for a replacement
4. Negotiates new pact, migrates data
5. Closes old pact only after new one is confirmed

### Data scope

For **Light node** pact partners, each pact covers events from the **latest checkpoint** onward — roughly a monthly window. Old data ages out of their pact obligations. **Full node** pact partners store complete event history for their peers. In both cases, the user's own devices hold full history, and archivists can opt into deeper storage via archival pacts.

*Keepers* (Full node pact partners) maintain 95% uptime. *Witnesses* (Light node pact partners) maintain 60% uptime in Phase 1 (browser extension/web) and 30% in Phase 2 (mobile). The 30% figure assumes active app usage. Mobile OS constraints (iOS BGAppRefreshTask, Android Doze) limit true background uptime to 0.3-5% depending on OS version and user behavior. See [Glossary](../glossary.md).

### Bootstrap pacts

New users have no WoT to form reciprocal pacts. The first person they follow becomes a temporary storage peer. See [ADR 006](../decisions/006-storage-pact-risk-mitigations.md).

- One-sided — the followed user stores the new user's data, no reciprocal obligation
- Auto-expires after 90 days or when the new user reaches 10 reciprocal pacts
- The followed user's client auto-accepts if capacity allows
- Transition: as the user builds WoT, bootstrap pacts phase out and reciprocal pacts take over

### Guardian pacts

An established user (*Guardian*) can volunteer to store data for one untrusted newcomer (*Seedling*) outside their Web of Trust. Guardian pacts complement bootstrap pacts — together they give newcomers two independent storage peers before forming any reciprocal pacts.

- One slot per Guardian — voluntary opt-in, no WoT required
- Kind 10053 with `type: guardian` tag
- Expiry: 90 days or Seedling reaches Hybrid phase (5+ reciprocal pacts)
- Challenge-response applies the same as any pact

| Phase | Storage Peers | Source |
|-------|--------------|--------|
| Seedling joins (0 pacts) | 0 | — |
| Guardian pact forms | 1 | Guardian volunteer |
| First follow → bootstrap pact | 2 | Followed user |
| WoT builds → reciprocal pacts | 3+ | WoT peers |
| 5+ pacts (Hybrid) | 5+ | Guardian expires |
| 10+ pacts | 10+ | Bootstrap expires |

See [ADR 006](../decisions/006-storage-pact-risk-mitigations.md) and the [protocol paper](../papers/gossip-storage-retrieval.md) §2.4, §4.7.

### Archival pacts

Standard pacts cover ~monthly windows. For long-term persistence, users can form archival pacts:

- Cover full history or a specified deep range
- Lower challenge frequency (weekly instead of daily)
- For power users, archivists, and users running always-on nodes
- Not mandatory — users without archival pacts are advised to run a persistent node

### Standby pacts

Maintain 3 extra pacts in standby mode to eliminate rebalancing delays:

- Standby peers receive events but aren't challenged or expected to serve
- When an active pact drops, promote a standby immediately — no discovery delay
- Backfill standby pool in the background

## Proof of Storage

Challenge-response protocol via kind 10054. Two challenge modes. See [ADR 006](../decisions/006-storage-pact-risk-mitigations.md).

**Hash challenge:** Alice sends Bob "hash events [47..53] with this nonce." Bob computes hash from local copy. Proves possession.

**Serve challenge:** Alice sends Bob "give me the full event at position 47." Measures response latency. Consistently slow responses suggest the peer is fetching remotely. Only used when both sides have direct, persistent connections (both 90%+ uptime). See [Pact Communication Matrix](#pact-communication-matrix).

**Completeness verification:** The checkpoint (kind 10051) includes a Merkle root of all events in the current window. Requesters compute the Merkle root from received events and compare against the checkpoint. Mismatch = events are missing. Light nodes additionally cross-verify the per-event hash chain for the most recent M events (default: 20) from each device — this catches checkpoint delegate censorship where a delegate omits sibling device events. See [ADR 008](../decisions/008-protocol-hardening.md).

**Reliability scoring:** Clients track a rolling 30-day reliability score per peer:

| Score | Status | Action |
|-------|--------|--------|
| 90%+ | Healthy | No action |
| 70–90% | Degraded | Increase challenge frequency |
| 50–70% | Unreliable | Begin replacement |
| < 50% | Failed | Drop immediately |

**Age-biased challenge distribution:** 50% of challenges target events from the oldest third of the stored range, 30% from the middle third, 20% from the newest third. This catches selective deletion of old data — a strategy where a peer keeps recent events (which are most frequently requested) while quietly discarding older events to save storage. Without age-biased distribution, a peer could maintain a >90% reliability score indefinitely by storing only the newest data. The bias ensures that old-data deletion degrades the reliability score at a rate proportional to the deletion.

**Failure handling:**
1. First failure → retry (network issues)
2. Second failure → ask other storage peers for same data
3. If others have it → failing peer's reliability score drops
4. Score below threshold → begin replacement negotiation
5. Natural consequence: failing peer loses reciprocal storage

**Channel-aware challenge retry:** When a challenge times out, the client MUST distinguish between "peer failed the challenge" and "communication channel failed." Before marking a challenge as failed:
1. Retry the challenge through an alternative relay (select from NIP-65 relay list or known mutual relays)
2. If the retry also fails, mark the challenge as failed and degrade the reliability score
3. If the retry succeeds, the original channel (relay) is flagged as unreliable — future challenges for this peer use the working relay

This prevents a malicious or unreliable relay from causing pact churn by dropping NIP-46 messages. The reliability score reflects the peer's actual data availability, not relay infrastructure failures.

### Network Partition Handling

Network partitions (country-level shutdowns, WoT community splits, relay outages) are handled explicitly:

**Detection:** A partition is suspected when multiple pact partners become simultaneously unreachable while the client's own connectivity to relays remains functional. Heuristic: if ≥3 pact partners in the same WoT cluster fail challenges within the same 1-hour window, treat as a likely partition rather than individual failures.

**During partition:**
- **Suspend reliability scoring** for pact partners in the suspected partition. Do not degrade their scores for challenge failures caused by connectivity loss.
- **Do not initiate pact replacement** for partition-affected peers. Standby pacts provide availability during the partition.
- **Continue operating** with remaining reachable pact partners and relay fallback.
- **Log partition events** (timestamps, affected peers) for post-partition reconciliation.

**After partition heals:**
- Resume challenge-response with previously partitioned peers.
- Reconcile events created during the partition via checkpoint comparison — each side may have published events the other did not receive.
- Restore reliability scores to pre-partition levels after 3 consecutive successful challenges.
- If a peer genuinely failed during the partition (not just unreachable), normal reliability scoring resumes and the peer is replaced through standard mechanisms.

**Pact set health (aggregate metric):** In addition to per-peer reliability, clients track the health of the full pact set:
- **Coverage score** — minimum number of online partners across all 24 hours (from uptime histograms). Target ≥ 3.
- **Overlap coefficient** — average pairwise uptime overlap across all partner pairs. Lower is better (more complementary coverage).
- **Functional balance** — ratio of Keepers to total active pacts. Target ≥ 15%.

When the coverage score drops below 3 or the Keeper ratio drops below 15%, the client prioritizes pact replacement or new pact formation to fill the gap — selecting partners whose uptime profile fills the identified coverage holes.

**Bounded timestamps:** Clients, relays, and storage peers reject events with `created_at` more than 15 minutes in the future. Events backdated more than 1 hour from the last known event from the same device are flagged. For replaceable event merge tiebreakers, timestamps within 60 seconds use lexicographic ordering of event ID (deterministic, non-gameable) instead of later-timestamp-wins.

## Event Retrieval

See [Data Flow](data-flow.md) for the full flow diagrams.

**Delivery paths (priority order):**
0. **Tier 0 — BLE mesh** — nearby devices serve events via Bluetooth. No internet required. Interoperable with [bitchat](https://github.com/permissionlesstech/bitchat). See [ADR 010](../decisions/010-bitchat-integration.md).
1. **Tier 1 — Cached endpoints** — follower has storage peer endpoints cached from kind 10059. Direct connection, zero broadcast overhead.
2. **Tier 2 — Gossip** — send kind 10057 to directly connected peers. Each peer forwards if they can't respond (TTL=3, reaches ~8,000 nodes in a 20-peer network).
3. **Tier 3 — Storage peers via DVM** — traditional kind 10057 broadcast through relay. Relay-dependent fallback.
4. **Tier 4 — Relays** — traditional relay query as last resort.

All paths produce self-authenticating events (signed by author's keys). Source doesn't matter — signatures prove authenticity.

**Content discovery beyond the WoT:** Content from authors outside the 2-hop WoT is discoverable through relay queries (Tier 4). Clients can subscribe to hashtag-filtered feeds from relays, which serve as curated discovery endpoints — relay operators select which content to index and surface. Standardized relay discovery APIs (compatible with NIP-11) enable clients to find relays serving specific topics or communities. This is intentionally relay-dependent: the WoT boundary provides spam resistance and storage efficiency for the pact layer, while relays serve the orthogonal function of broad content discovery.

### Gossip Hardening

Gossip forwarding (kind 10055, 10057) is hardened against amplification attacks. All hardening rules are enforced **client-side** — no relay modifications needed. See [ADR 008](../decisions/008-protocol-hardening.md).

**Per-hop rate limiting:** Each client enforces a maximum request rate per source pubkey:
- Kind 10055 (pact request): 10 req/s per source
- Kind 10057 (data request): 50 req/s per source
- Excess requests are dropped silently

**Request deduplication:** Each request carries a `request_id` tag. Clients track seen request_ids in an LRU cache (10,000 entries). Duplicate requests are dropped.

**WoT-only forwarding:** Clients only forward gossip from pubkeys within their 2-hop WoT. Requests from unknown pubkeys are served locally (if possible) but never forwarded. This bounds the gossip blast radius to the WoT graph.

**Gossip topology exposure:** Gossip forwarding reveals the network graph to observers at multiple nodes. The WoT-only forwarding rule (client-enforced) limits exposure to WoT members. Onion routing for gossip requests is a potential future enhancement (see [ADR 008](../decisions/008-protocol-hardening.md), §12) but is not part of the current protocol — it requires substantial design work around route construction, exit node authentication, and circuit correlation prevention.

## Privacy

- **Pacts are private** — kind 10053 exchanged directly, never published
- **Topology is hidden** — no public list of who stores whose data
- **Endpoint hints are gift-wrapped** — kind 10059 wrapped in NIP-59, relay stores opaque blob, only intended follower decrypts
- **Retrieval is per-request** — storage peers reveal themselves only to individual requesters via kind 10058
- **Peers can filter** — respond only to WoT members, or not at all
- **Pseudonymous data requests via rotating request tokens** — kind 10057 uses a rotating request token `H(target_pubkey || YYYY-MM-DD)` instead of raw pubkey — computed as H(target_pubkey || YYYY-MM-DD), a daily-rotating lookup key that prevents casual cross-day linkage but is reversible by any party knowing the target's public key. Not a formal cryptographic blinding scheme. Storage peers match against both today's and yesterday's date to handle clock skew at day boundaries (dual-day token matching). See [ADR 008](../decisions/008-protocol-hardening.md).
- **DM integrity** — NIP-44 uses AEAD (authenticated encryption). Storage peers hold encrypted DM blobs but cannot serve corrupted ciphertext — tampered ciphertext fails decryption with an authentication error. The existing challenge-response proves possession; AEAD proves integrity.

## Peer Selection

Client-side rules for choosing storage peers. See [ADR 006](../decisions/006-storage-pact-risk-mitigations.md).

**WoT cluster diversity:** Maximum 3 peers from any single social cluster. At least 4 distinct clusters across 20 peers. Prevents eclipse attacks where an attacker becomes a majority of your storage peers.

**Geographic diversity:** Target 3+ timezone bands. Never more than 50% of peers in the same ±3 hour band. Protects against correlated regional failures.

**Uptime complementarity:** Peer selection should minimize uptime overlap across the pact set. Two peers who are both online 9am-5pm and both offline at night provide redundant coverage — "destructive interference" for availability. Two peers with anti-correlated schedules (one online during the other's offline hours) provide complementary coverage — "constructive interference."

Clients compute a **pact set coverage score**: for each hour of the day, count how many pact partners are typically online (derived from challenge-response timestamps over a rolling 7-day window). The coverage score is the minimum hourly count across all 24 hours. A score of 0 means there exists an hour where no pact partner is typically online — a coverage gap. Target: coverage score ≥ 3 (at least 3 partners online during every hour). When evaluating new pact offers (kind 10056), prefer partners whose uptime pattern fills existing coverage gaps over partners who overlap with existing peers.

This catches a failure mode that geographic diversity alone misses: peers in adjacent timezones (technically satisfying the "3+ bands" rule) but with 80%+ uptime overlap due to similar work schedules.

**Functional diversity:** Require a minimum of 3 Keepers (full nodes, 90%+ uptime) among active pact partners. The remaining pacts can be Witnesses (light nodes). A pact set with zero Keepers has no always-on storage — all data is unavailable during overnight hours when Witnesses sleep. Scale the minimum Keeper count with total pact count: 3 for 10 pacts, 5 for 20 pacts, 8 for 30+ pacts.

**Follow-age requirement:** Pact partners must have a mutual follow relationship of at least 30 days. This prevents gaming WoT edges instrumentally for pact access — an attacker cannot create a fresh identity, follow a target, and immediately form a reciprocal pact. The 30-day window ensures that pact relationships reflect genuine social connections rather than strategic positioning. Bootstrap pacts (which are one-sided and temporary) are exempt from this requirement to avoid blocking new user onboarding.

**Peer reputation:** Weight offers by identity age, challenge success rate, and active pact count. Identities < 30 days old are limited to bootstrap pacts.

**Equilibrium-seeking formation:** The protocol does not prescribe a fixed pact count. Instead, it forms pacts until a measurable comfort condition is satisfied: ∀h ∈ {0..23}: P(X_h < 3) ≤ 0.001, where X_h is the Poisson-binomial-distributed count of online partners at hour h. The equilibrium count emerges from the user's specific partner mix (all Keepers ≈ 7, mixed ≈ 14–20, all Witnesses ≈ 33–40). A PACT_FLOOR of 12 ensures Keepers accept beyond their own comfort, providing availability for Witnesses. PACT_CEILING is 40. See [Equilibrium Pact Formation](../design/equilibrium-pact-formation.md) for the full model.

**Offer filtering:** Drop offers from non-WoT pubkeys, identities < 30 days old, or volume mismatch > 50%.

## Platform Focus

> **Phase 1 targets desktop and web (browser extension).** Mobile support is designed into the protocol but not the current implementation target.

**Desktop (Full node, 95% uptime):** Always-on servers and desktop apps. Direct WebSocket connections to pact partners. `serve` + `hash` challenges with fast response windows. Primary pact endpoint.

**Browser extension (Light node, 60% uptime):** Active when the browser is open. Maintains WebSocket connections via service worker. `hash`-only challenges with 4-8h response windows. Relay-mediated sync when browser is closed.

**Device-aware routing:** The protocol discovers a user's device fleet via Kind 10050 uptime tags and automatically routes pact obligations to the most reliable device. A desktop at 91% uptime becomes the primary pact endpoint; the extension syncs through it.

**Mobile (Phase 2):** The protocol supports mobile-to-mobile pacts via relay-mediated mailbox sync with 24h async challenge windows. See [Platform Architecture](platform-architecture.md) §3 for the full mobile design.

## Pact Communication Matrix

> **Phase 1 targets Full ↔ Full and Full ↔ Active pairs** (desktop apps and browser extensions). Intermittent pairs (mobile) are Phase 2.

Pact behavior adapts automatically to the **weaker** device in the pair. Each client reads its partner's Kind 10050 uptime tags and selects the appropriate communication mode — no negotiation needed.

Device classifications from uptime tags: **Full node** (90%+, *Keeper*), **Active** (50-89%, browser extension), **Intermittent** (10-49%, mobile — Phase 2), **Passive** (<10%). See [Platform Architecture](platform-architecture.md) for details.

### Connection Method

| Pair | Connection | Notes |
|------|-----------|-------|
| Full ↔ Full | Direct WebSocket or UDP (NAT hole punch) | Both have stable IPs; relay only needed for signaling |
| Full ↔ Active | Direct or relay | Active device connects outbound to full node when online |
| Full ↔ Intermittent | Relay | Full node always listening; intermittent device queries `REQ since:` on wake |
| Active ↔ Active | Direct (NAT hole punch) or relay | Hole punch when both online; relay stores events otherwise |
| Active ↔ Intermittent | Relay | Relay stores events for intermittent device |
| Intermittent ↔ Intermittent | Relay | Both query `REQ since:` on wake; checkpoint reconciliation as safety net |

When a direct connection is available (both sides online with stable endpoints), the protocol uses it. When either side is behind NAT or offline, the relay handles event storage and delivery via standard Nostr subscriptions. See [NAT Hole Punching](../actors/relay.md#nat-hole-punching-optional) for how relays can help establish direct connections.

### Challenge Model

The challenge type and response window are determined by the weaker device in the pair:

| Pair | Challenge type | Response window | Rationale |
|------|---------------|----------------|-----------|
| Full ↔ Full | `serve` + `hash` | 500ms (serve), 1h (hash) | Both persistent — latency test detects proxy-fetching |
| Full ↔ Active | `hash` only | 4h | Active device may restart (browser extension); latency test meaningless |
| Full ↔ Intermittent | `hash` only | 24h | Intermittent device online sporadically |
| Active ↔ Active | `hash` only | 8h | Both may be offline for hours |
| Active ↔ Intermittent | `hash` only | 24h | Window sized to intermittent device |
| Intermittent ↔ Intermittent | `hash` only | 24h | Widest window; checkpoint reconciliation supplements |

**Why `serve` challenges require both sides to be full nodes:** The `serve` challenge measures response latency to detect peers that claim to store data locally but actually fetch it remotely. This only works when the responding device has a persistent, direct connection — if it's behind a relay or offline for hours, slow responses are expected and prove nothing.

### Event Delivery

| Pair | Delivery mode | Sync pattern |
|------|-------------|--------------|
| Full ↔ Full | Real-time push (persistent WebSocket or UDP) | Continuous bidirectional stream |
| Full ↔ Active | Push when connected; stored on relay when not | Batch on wake → then stream while active |
| Full ↔ Intermittent | Relay-stored | Batch on wake via `REQ since:` |
| Active ↔ Active | Relay subscription when both active | Relay `REQ since:` on wake |
| Active ↔ Intermittent | Relay-stored | Relay `REQ since:` on wake |
| Intermittent ↔ Intermittent | Relay-stored | Relay `REQ since:` on wake; checkpoint reconciliation as safety net |

### How Clients Determine the Pair Mode

No negotiation protocol. Each client reads its partner's Kind 10050 and autonomously selects behavior:

1. Fetch partner's Kind 10050 → find their highest-uptime device
2. Classify partner's best device: Full (90%+), Active (50-89%), Intermittent (10-49%), Passive (<10%)
3. Classify own best device the same way
4. The pair mode = min(own classification, partner classification)
5. Select challenge type, response window, and delivery mode from the matrix above

This runs on every Kind 10050 update (at most daily). If a partner's desktop goes offline and their uptime drops from 91% to 40%, the pair mode automatically shifts from Full↔Full to Active↔Intermittent — longer challenge windows, relay-mediated delivery. No renegotiation needed.

## Relay Role

Standard Nostr relays work with Gozzip without any code changes. All protocol intelligence (gossip forwarding, rotating request token matching, WoT filtering, device resolution) lives in clients. The relay stores events and serves subscriptions — exactly what it already does.

- **No relay modifications required** — every Gozzip event kind is a valid Nostr event
- **Mobile "mailbox" is standard relay storage** — phones query `since: <last_timestamp>` on reconnect
- **NIP-65 relay lists** enable discovery and failover — unreliable relays get dropped
- **NIP-59 gift wrapping** handles privacy for Kind 10059 endpoint hints — relay stores opaque blobs
- Clients try storage peers and relays opportunistically
- No single relay failure can make a user's data unavailable
- Gradual migration — relays work alongside storage peers

Relays serve as delivery infrastructure with reduced data custody. While the protocol progressively reduces relay dependence for data *storage* (events survive on pact partners), relays remain structurally important for: new user bootstrap, content discovery beyond the WoT, mobile-to-mobile pact communication (relay as mailbox), and push notification delivery. Optional relay optimizations (oracle resolution, checkpoint delta, gossip relay forwarding) can accelerate performance but are never required. See [Relay](../actors/relay.md).

**Relay economics during transition:** The protocol reduces relay data custody revenue (fewer users depend on relays as their sole data store) while relays remain structurally necessary for delivery and discovery. A viable economic model for the transition: relay-as-Keeper integration, where relay operators run their relays as full Gozzip nodes that form pacts with users. In this model, the relay earns pact-aware gossip priority (wider content distribution) and Lightning payments (see [Incentive Model](../design/incentive-model.md)) while providing the high-uptime, always-on storage that the pact network benefits from. Relays that integrate as Keepers become first-class participants in the storage mesh rather than legacy infrastructure being displaced.

## Three-Phase Adoption Model

Client behavior adapts automatically based on the user's pact count:

| Phase | Pact count | Behavior |
|-------|-----------|----------|
| Bootstrap | 0–5 pacts | Publish to relays primarily. Form pacts as available. |
| Hybrid | 5–15 pacts | Publish to both relays and storage peers. Fetch from peers first, relay fallback. |
| Sovereign | 15+ pacts | Storage peers primary. Relays serve as delivery infrastructure with reduced data custody. |

Transition is automatic and per-user — no network-wide flag. Early adopters stay in bootstrap phase longer. As the network grows, users transition naturally.

No protocol changes — this is client-side logic controlling which delivery path to prioritize.

## Cascading Read-Caches

Popular accounts would overwhelm their storage peers (10–40+, depending on follower count) with serving load. Read-caches solve this by turning followers into voluntary data mirrors.

1. Bob fetches Alice's events from storage peer S1
2. Bob now has a local copy
3. Carol broadcasts kind 10057 for Alice's data
4. Bob's client sees the request and responds (he has Alice's events)
5. Carol verifies Alice's signatures — source doesn't matter

**Properties:**
- No pact needed — Bob serves cached data he already has
- Self-authenticating events — signatures prove integrity regardless of source
- Popular data naturally replicates across the follower base
- Storage peers handle first wave; followers' caches handle the tail
- Load scales with O(followers), not O(storage_peers)

**Client configuration:**
- `read_cache_enabled` — whether to serve cached data for followed users (default: true)
- `read_cache_max_mb` — storage limit for cached data (default: 100MB)
- `read_cache_respond_to` — who to serve: `wot_only` (default) or `anyone`

No new event kinds — uses existing kind 10057/10058 flow.

### Tiered Cache TTLs

Non-pact cached content expires based on the feed tier it was fetched through. See [Feed Model](feed-model.md) for tier definitions.

| Feed Tier | Default TTL | Config Key |
|-----------|------------|------------|
| Inner Circle | 30 days | (pact-managed) |
| Orbit | 14 days | `orbit_cache_ttl` |
| Horizon | 3 days | `horizon_cache_ttl` |
| Relay-only | 1 day | `relay_cache_ttl` |

Eviction strategy: LRU within TTL boundary, bounded by `read_cache_max_mb`. Pact-covered content (Inner Circle) is not subject to cache eviction — it's managed by pact obligations. All cached content, regardless of tier or remaining TTL, is served in response to gossip requests.

## Media Storage

Media (images, video, audio) is handled separately from event pacts. Events contain content-addressed hash references (`media` tags) to media blobs stored externally — on CDNs, S3, IPFS, or self-hosted servers. The event itself remains ~1 KB regardless of how many media files it references.

**Key separation:** Event pact obligations (kind 10053 with `type: standard`) cover events only. Media volume is excluded from event pact volume matching and challenges. This keeps pact obligations tractable — a user with 100 MB of events and 5 GB of media has a 100 MB event pact volume.

**Optional media pacts:** Users who want peer-to-peer media redundancy can form media pacts (kind 10053 with `type: media`) — separate volume accounting, separate challenges (random byte-range instead of full-file hash), and a configurable retention window (default: 90 days). Media pacts are restricted to Keepers (full nodes, 90%+ uptime). Mobile devices and browser extensions are never obligated to store or serve media for pact partners.

**Integrity:** Clients verify `SHA-256(downloaded_bytes) == hash_from_media_tag` on every fetch. Source does not matter — a CDN, IPFS node, or pact partner serving the correct bytes is equally trusted.

See [Media Layer](../design/media-layer.md) for the full design.

## Incentive Model

Storage contribution translates directly to content reach through pact-aware gossip routing. No explicit scores, tokens, or subscriptions — the network topology IS the incentive. See [ADR 009](../decisions/009-incentive-model.md).

### Pact-Aware Gossip Priority

When a node forwards gossip or decides what content to propagate, it applies priority ordering:

1. **Active pact partners** — highest priority. You store their data, you forward their content eagerly. Self-interested: if their content is discoverable, your storage pact has value (others can find the data you're holding).
2. **WoT contacts** (1-hop follows) — standard priority.
3. **Extended WoT** (2-hop) — lower priority, forwarded if capacity allows.
4. **Unknown pubkeys** — served locally if available, never forwarded (existing gossip hardening rule).

A user with 20 reliable pact partners has 20 nodes that eagerly forward their content. A user with 5 flaky pacts has fewer advocates.

### Free-Rider Resilience

The cooperative equilibrium is fragile above approximately 30% defection — if more than 30% of nodes free-ride (accepting storage from pact partners without reliably storing in return), the incentive to cooperate degrades for remaining honest nodes. Defection is made observable through the reliability scoring system: peers whose challenge pass rate drops are visible to their pact partners, who can then replace them. Tiered service reinforces cooperation — nodes with higher pact contribution (more active pacts, higher reliability scores) receive higher gossip forwarding priority, creating a measurable reach advantage for cooperators over defectors.

### Natural Consequences

- **Dropped pact** — the dropped peer loses a forwarding advocate. Their content distribution naturally shrinks.
- **Reliable storage** — maintained pacts = maintained reach. The incentive to store reliably is self-interested.
- **No cliff** — there's no minimum contribution to participate. Less contribution means less reach, not exclusion. The network degrades gracefully.
