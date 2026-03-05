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

*Keepers* (Full node pact partners) maintain 95% uptime. *Witnesses* (Light node pact partners) maintain 60% uptime in Phase 1 (browser extension/web) and 30% in Phase 2 (mobile). See [Glossary](../glossary.md).

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

**Failure handling:**
1. First failure → retry (network issues)
2. Second failure → ask other storage peers for same data
3. If others have it → failing peer's reliability score drops
4. Score below threshold → begin replacement negotiation
5. Natural consequence: failing peer loses reciprocal storage

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

### Gossip Hardening

Gossip forwarding (kind 10055, 10057) is hardened against amplification attacks. All hardening rules are enforced **client-side** — no relay modifications needed. See [ADR 008](../decisions/008-protocol-hardening.md).

**Per-hop rate limiting:** Each client enforces a maximum request rate per source pubkey:
- Kind 10055 (pact request): 10 req/s per source
- Kind 10057 (data request): 50 req/s per source
- Excess requests are dropped silently

**Request deduplication:** Each request carries a `request_id` tag. Clients track seen request_ids in an LRU cache (10,000 entries). Duplicate requests are dropped.

**WoT-only forwarding:** Clients only forward gossip from pubkeys within their 2-hop WoT. Requests from unknown pubkeys are served locally (if possible) but never forwarded. This bounds the gossip blast radius to the WoT graph.

**Gossip topology exposure:** Gossip forwarding reveals the network graph to observers at multiple nodes. The WoT-only forwarding rule (client-enforced) limits exposure to WoT members. Relays can offer onion routing as an optional premium service — gossip requests wrapped in NIP-44 encrypted layers per hop, hiding the request path from intermediate nodes. Users subscribe to onion routing via Lightning zaps. See [relay Lightning services](../actors/relay.md#lightning-services).

## Privacy

- **Pacts are private** — kind 10053 exchanged directly, never published
- **Topology is hidden** — no public list of who stores whose data
- **Endpoint hints are gift-wrapped** — kind 10059 wrapped in NIP-59, relay stores opaque blob, only intended follower decrypts
- **Retrieval is per-request** — storage peers reveal themselves only to individual requesters via kind 10058
- **Peers can filter** — respond only to WoT members, or not at all
- **Blinded data requests** — kind 10057 uses `H(target_pubkey || daily_salt)` instead of raw pubkey. Observers can't identify whose data is being requested or link requests across days. Storage peers match against both today and yesterday's date to handle clock skew at day boundaries (dual-day blind matching). See [ADR 008](../decisions/008-protocol-hardening.md).
- **DM integrity** — NIP-44 uses AEAD (authenticated encryption). Storage peers hold encrypted DM blobs but cannot serve corrupted ciphertext — tampered ciphertext fails decryption with an authentication error. The existing challenge-response proves possession; AEAD proves integrity.

## Peer Selection

Client-side rules for choosing storage peers. See [ADR 006](../decisions/006-storage-pact-risk-mitigations.md).

**WoT cluster diversity:** Maximum 3 peers from any single social cluster. At least 4 distinct clusters across 20 peers. Prevents eclipse attacks where an attacker becomes a majority of your storage peers.

**Geographic diversity:** Target 3+ timezone bands. Never more than 50% of peers in the same ±3 hour band. Protects against correlated regional failures.

**Peer reputation:** Weight offers by identity age, challenge success rate, and active pact count. Identities < 30 days old are limited to bootstrap pacts.

**Popularity scaling:** Scale pact count with follower count (< 100 followers → 10 pacts, 1,000+ → 30 pacts, 10,000+ → 40+). More pacts = more peers sharing serving load.

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

Standard Nostr relays work with Gozzip without any code changes. All protocol intelligence (gossip forwarding, blinded matching, WoT filtering, device resolution) lives in clients. The relay stores events and serves subscriptions — exactly what it already does.

- **No relay modifications required** — every Gozzip event kind is a valid Nostr event
- **Mobile "mailbox" is standard relay storage** — phones query `since: <last_timestamp>` on reconnect
- **NIP-65 relay lists** enable discovery and failover — unreliable relays get dropped
- **NIP-59 gift wrapping** handles privacy for Kind 10059 endpoint hints — relay stores opaque blobs
- Clients try storage peers and relays opportunistically
- No single relay failure can make a user's data unavailable
- Gradual migration — relays work alongside storage peers

Optional relay optimizations (oracle resolution, checkpoint delta, gossip relay forwarding) can accelerate performance but are never required. See [Relay](../actors/relay.md).

## Three-Phase Adoption Model

Client behavior adapts automatically based on the user's pact count:

| Phase | Pact count | Behavior |
|-------|-----------|----------|
| Bootstrap | 0–5 pacts | Publish to relays primarily. Form pacts as available. |
| Hybrid | 5–15 pacts | Publish to both relays and storage peers. Fetch from peers first, relay fallback. |
| Sovereign | 15+ pacts | Storage peers primary. Relays optional accelerator. |

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

## Incentive Model

Storage contribution translates directly to content reach through pact-aware gossip routing. No explicit scores, tokens, or subscriptions — the network topology IS the incentive. See [ADR 009](../decisions/009-incentive-model.md).

### Pact-Aware Gossip Priority

When a node forwards gossip or decides what content to propagate, it applies priority ordering:

1. **Active pact partners** — highest priority. You store their data, you forward their content eagerly. Self-interested: if their content is discoverable, your storage pact has value (others can find the data you're holding).
2. **WoT contacts** (1-hop follows) — standard priority.
3. **Extended WoT** (2-hop) — lower priority, forwarded if capacity allows.
4. **Unknown pubkeys** — served locally if available, never forwarded (existing gossip hardening rule).

A user with 20 reliable pact partners has 20 nodes that eagerly forward their content. A user with 5 flaky pacts has fewer advocates.

### Natural Consequences

- **Dropped pact** — the dropped peer loses a forwarding advocate. Their content distribution naturally shrinks.
- **Reliable storage** — maintained pacts = maintained reach. The incentive to store reliably is self-interested.
- **No cliff** — there's no minimum contribution to participate. Less contribution means less reach, not exclusion. The network degrades gracefully.
