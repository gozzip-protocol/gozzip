# Adversarial Review: Unnecessary Complexity

**Reviewer perspective:** Systems architect, 3-engineer team, 12-month delivery target
**Date:** 2026-03-14
**Status:** Complete

---

## Executive Summary

The Gozzip protocol is over-engineered for its current stage. The whitepaper describes a system with 19 custom event kinds (10050-10068), 7+ design documents totaling ~50,000 words of specification, a 7-state pact FSM, a 5-tier retrieval cascade, an equilibrium-seeking formation model requiring Poisson binomial convolution, media pacts as a separate subsystem, interplanetary compatibility analysis, protocol versioning with deprecation timelines, federated blocklist infrastructure, CSAM hash-matching pipelines, and a four-phase deployment roadmap.

No team of three engineers can build this in twelve months. No protocol at zero users needs this much specification. The complexity is not wrong in the abstract -- most of it is well-reasoned -- but it is wrong *right now*. The protocol needs 80% fewer features and 80% less specification to ship a usable product.

This review identifies what to cut entirely, what to simplify dramatically, and what the minimal viable protocol looks like -- the smallest thing that delivers the core value proposition of "your data lives on your social graph, not on servers."

---

## Components to Cut Entirely

### 1. Interplanetary Compatibility (interplanetary.md)

**Current cost:** A full design document, a subsection in the whitepaper (Section 8.7), attack surface analysis for WoT bridge compromise, latency-adapted timeout tables, small-community pact scaling analysis.

**Why cut it:** The document itself says "this is not an interplanetary protocol" and "Gozzip is not an interplanetary protocol." It then spends 3,000 words analyzing how it would work as one. The honest assessment at the bottom -- "bake planetary routing into the core protocol would be premature optimization for a scenario that may never materialize" -- is correct. Follow that advice. Delete the document. Remove the whitepaper section. If Mars colonists need a social protocol in 2040, they can write an extension RFC.

**Savings:** One design document eliminated. ~1,500 words removed from the whitepaper. Zero engineering effort allocated to DTN transport adapters.

### 2. Protocol Versioning (protocol-versioning.md at current depth)

**Current cost:** A 400-line design document specifying version negotiation, `supported_versions` tags on pact requests, `negotiated_version` tags on pact events, mid-pact upgrades, deprecation timelines (N, N-1, N-2, N-3), security update handling for hash algorithm weakening, key derivation changes, emergency minimum version bumps, database schema migration guidelines, NIP registry with 11 NIP groupings, compatibility matrices, and test vector specifications.

**Why cut it:** There is exactly one protocol version. There are zero deployed clients. Version negotiation is needed when you have two versions and users running both. That situation is 18-24 months away at minimum. The entire document solves a problem that does not yet exist.

**What to keep:** The `protocol_version` tag on events (already specified in the whitepaper). A single sentence: "All custom event kinds include a `protocol_version` tag set to `"1"`. Versioning strategy will be defined before the first breaking change."

**Savings:** One design document deferred entirely. No negotiation logic, no compatibility matrices, no deprecation timelines to implement. Hundreds of lines of client code not written.

### 3. Media Pacts as a Separate Subsystem (media-layer.md, pact type separation)

**Current cost:** A 675-line design document. Separate pact type (`type: media`), separate volume matching with different tolerance (+/-100% vs +/-30%), separate challenge type (`media_range` with byte-range verification), Keeper-only constraint, separate media manifest event (kind 10065), 6-tier media retrieval cascade, relay CDN integration with three economic models, mobile bandwidth budgeting, independent pact health tracking.

**Why cut it:** Media handling is important, but media *pacts* are not a v1 feature. The document itself acknowledges the correct v1 approach: "Most users will rely on relay CDN for media. This is an acceptable tradeoff." Content-addressed media references (the `media` tag with SHA-256 hash) are simple, correct, and sufficient. Users upload to CDNs, IPFS, or self-hosted servers. Clients verify the hash. Done.

Media pacts add a second pact economy with different matching rules, different challenges, different device eligibility, different volume calculations, and different health tracking -- essentially doubling the pact subsystem's implementation surface. For v1, the hash-reference approach gives 95% of the value with 5% of the complexity.

**What to keep:** The `media` and `media_thumb` tags on events. SHA-256 integrity verification on fetch. Local media cache with LRU eviction. The tiered retrieval for media (local cache, url_hint, relay CDN). Delete kind 10065, delete media pacts, delete byte-range challenges.

**Savings:** Kind 10065 eliminated. Media pact formation, challenge, and health tracking not implemented. No separate volume matching for media. No Keeper-only constraint logic for media. Roughly 40% of pact subsystem complexity eliminated.

### 4. Blocklist Federation (moderation.md Section 3, kind 10070)

**Current cost:** A new event kind (10070), subscription model via kind 30078, community-maintained list infrastructure, relay operator integration for list-based deny lists, transparency model, no-default-blocklists policy discussion.

**Why cut it:** NIP-51 mute lists already exist in Nostr. Users can mute pubkeys. Relay operators already have deny lists. Federated blocklists are a governance feature for a mature network with thousands of users and active moderation communities. At zero users, there is nobody to block and no community to maintain a blocklist.

**What to keep:** NIP-51 mute lists (already in Nostr, no implementation needed). Kind 10064 content reports (simple, useful from day one). NIP-32 label integration (existing Nostr infrastructure).

**Savings:** Kind 10070 eliminated. No subscription model. No list federation logic.

### 5. Notification Relay Advertisement (kind 10067)

**Current cost:** A new event kind for notification relay discovery, platform tags, NIP-46 relay tags, operator metadata.

**Why cut it:** At launch, there will be 1-2 notification relays, hardcoded or configured manually. Discovery of notification relays via a NIP event is a scaling feature for a network with dozens of competing notification relay operators. That is years away.

**What to keep:** Kind 10062 (push registration). Hardcode the notification relay URL in the client configuration.

**Savings:** Kind 10067 eliminated. No discovery infrastructure.

### 6. Wake-Up Hint via Pact Partners (kind 20062)

**Current cost:** A new ephemeral event kind, pact-partner-as-notification-proxy flow, verification that the sender is a known pact partner, reduced notification relay dependency path.

**Why cut it:** This optimizes the push notification path by routing through pact partners instead of having the notification relay subscribe directly. It is an optimization of an optimization. The standard flow (notification relay subscribes to user's events on NIP-65 relays, sends push on match) works fine. Adding an intermediate hop through pact partners reduces metadata exposure marginally but adds significant implementation complexity.

**What to keep:** Direct notification relay subscription model.

**Savings:** Kind 20062 eliminated. No pact-partner proxy logic for push notifications.

### 7. Guardianship Completion Event (kind 10066)

**Current cost:** A new event kind for mutual attestation of successful guardianship.

**Why cut it:** Guardian pacts already have clear expiry conditions (90 days or Seedling reaches Hybrid phase). A completion event is a nice-to-have social signal ("I was a guardian, here's proof"). It is not needed for protocol operation. The guardian pact simply transitions to Dropped when the expiry condition is met.

**What to keep:** Guardian pact expiry logic (already in the pact state machine).

**Savings:** Kind 10066 eliminated. Marginal but reduces the event kind surface.

---

## Components to Simplify

### 1. Pact State Machine: 7 States to 4 States

**Current complexity:** 7 states (Pending, Offered, Active, Standby, Degraded, Failed, Dropped) with 12+ transitions, partition handling, renegotiation, guardian pact lifecycle modifications.

**Proposed simplification:** 4 states: **Forming**, **Active**, **Failing**, **Ended**.

| Current | Simplified | Notes |
|---------|-----------|-------|
| Pending + Offered | Forming | Merge negotiation states. Client tracks offers internally; the protocol only needs "not yet active." |
| Active + Standby | Active | Drop the standby concept for v1. All pacts are active. Challenge all partners. The distinction between "challenged" and "not challenged" is an optimization for reducing NIP-46 traffic, not a correctness requirement. |
| Degraded + Failed | Failing | Merge into a single "this pact is in trouble" state. Below 70% reliability, seek replacement. Below 50%, drop immediately. No need for two separate failure-track states. |
| Dropped + Rejected | Ended | Terminal state. Pact is over. |

**What this eliminates:**
- Standby-to-Active promotion logic (all pacts are active)
- Standby unresponsive timeout (7 days) vs. Active unresponsive timeout (72 hours) distinction
- Over-Provisioned state and pact dissolution logic
- Degraded-to-Active recovery hysteresis (degrade at 70%, recover at 80%)
- The concept of "standby pacts" entirely

**What this costs:** Slightly higher NIP-46 challenge traffic (challenging all partners instead of only active ones). Slightly slower failover when a partner goes down (must form a new pact rather than promoting a standby). Both are acceptable for v1.

### 2. Retrieval Cascade: 5 Tiers to 3 Tiers

**Current complexity:** Tier 0 (BLE mesh), Tier 1 (local/instant), Tier 2 (cached endpoints), Tier 3 (gossip with rotating tokens), Tier 4 (relay fallback).

**Proposed simplification:** 3 tiers: **Local**, **Gossip**, **Relay**.

| Current | Simplified | Notes |
|---------|-----------|-------|
| Tier 0 (BLE mesh) | Cut for v1 | BLE mesh requires FIPS integration, BitChat interop, Noise Protocol sessions, Bloom filter dedup, geohash discovery. This is Phase 4 functionality. |
| Tier 1 (local/instant) | **Local** | Already stored from a pact partner. Zero network cost. |
| Tier 2 (cached endpoints) | Merge into Local | Cached endpoint addresses are a performance optimization. For v1, if you have the data locally, serve it. If not, gossip. |
| Tier 3 (gossip) | **Gossip** | Broadcast data request to WoT peers with TTL=3. |
| Tier 4 (relay fallback) | **Relay** | Standard Nostr relay query. |

**What this eliminates:**
- BLE mesh transport (entire FIPS dependency for v1)
- Cached endpoint events (kind 10059)
- Endpoint hint caching and management
- The complexity of maintaining 5 separate retrieval paths with fallback logic

**What this costs:** No offline/BLE operation (acceptable for v1). Slightly slower retrieval in cases where a cached endpoint would have been faster than gossip (marginal; simulation shows 74-92% of reads are instant/local anyway).

### 3. Equilibrium-Seeking Formation: Replace with Fixed Target

**Current complexity:** Poisson binomial distribution computation, per-hour comfort condition evaluation ($P(X_h < K) \leq \varepsilon$ for all 24 hours), Berry-Esseen bound considerations, exact convolution via O(n^2) dynamic programming, PACT_FLOOR of 12, PACT_CEILING of 40, marginal value computation, functional diversity constraint (15% Keeper ratio), uptime complementarity scoring across 24-hour windows, correlated failure modeling via beta-binomial distribution, formation state machine (Bootstrap/Growing/Comfortable/Over-Provisioned/Degraded), hysteresis with 10x gap between thresholds.

**Proposed simplification:** Target 15 active pacts. Keep forming until you reach 15. If one drops, form another. Stop.

**What to keep:** Volume matching (30% tolerance). WoT membership requirement. Account age requirement (7 days). The qualitative intuition that you want diverse partners (different timezones, mix of full/light nodes). But implement diversity as a heuristic ("prefer partners in different timezones"), not as a formal optimization over uptime histograms.

**What this eliminates:**
- Poisson binomial computation
- 24-hour per-hour comfort condition
- PACT_FLOOR/PACT_CEILING logic
- Marginal value computation
- Functional diversity ratio enforcement
- Uptime complementarity scoring
- Beta-binomial correlated failure modeling
- Formation state machine (5 states)
- Hysteresis thresholds

**What this costs:** Some users may have slightly suboptimal pact sets (too many partners in the same timezone, not enough Keepers). Acceptable for v1. The simulation already shows 94-98% availability with much simpler models.

### 4. Monitoring and Diagnostics: Cut to Essentials

**Current complexity:** 8 sections covering pact health dashboard, gossip propagation debugging, relay interaction logging, challenge-response diagnostics, WoT visualization (force-directed graph with community detection), health check command (6-test suite), protocol-level tracing (developer mode with circular buffer), relay compatibility reporting (9-requirement test suite with scoring).

**Proposed simplification:** Ship two things:

1. **Pact list with health indicators.** For each pact: partner name, online/offline status, challenge pass rate (last 10), pact age. Color code green/yellow/red. That is the entire pact health dashboard.

2. **Simple health check.** Ping pact partners, check relay connectivity. Report "X/Y partners reachable, Z/W relays connected." No compatibility scoring, no WoT visualization, no gossip trace, no developer mode tracing.

Everything else (WoT visualization, gossip propagation debugging, relay compatibility scoring, protocol-level tracing, community detection) is deferred to post-launch.

### 5. GDPR Deletion: Simplify to Core

**Current complexity:** Kind 10063 with selective and full deletion, 72-hour pact partner compliance window, verification via challenge-response, enforcement via pact degradation, read cache forced eviction, BLE mesh copy analysis, full account deletion sequence (4 steps), legal analysis (GDPR roles, lawful basis, DPIA requirements), 30-day auxiliary data purge, tombstone retention.

**Proposed simplification:** Kind 10063 deletion request. Partners delete the referenced events. If they do not, their next challenge will fail (the hash will not match), and normal reliability scoring handles it. No separate deletion verification, no 72-hour window specification, no auxiliary data purge timeline. The existing challenge-response mechanism already detects missing data.

Account deletion: publish kind 10063 with `all_events`, drop all pacts, revoke device keys. Three steps, not a formal sequence with partner obligation timelines.

Defer the legal analysis (DPIA, controller-vs-processor classification, lawful basis analysis) to a separate legal document produced by actual legal counsel before EU launch. A protocol spec should not contain legal analysis.

### 6. Push Notifications: Cut to MVP

**Current complexity:** 4 platform support (APNs, FCM, UnifiedPush, Web Push), NIP-46 authenticated registration, pact-partner-as-notification-proxy flow, kind 10067 discovery, kind 20062 wake-up hints, quiet hours with timezone offsets, batching windows with cooldown periods, daily caps, filter minimization for metadata exposure mitigation, multiple relay registration for redundancy, token rotation schedules.

**Proposed simplification:** Support APNs and FCM only for v1 (covers 99% of mobile users). Single notification relay, hardcoded. Kind 10062 registration with encrypted push token and basic filters (DMs: yes/no, mentions: yes/no). No quiet hours, no batching configuration, no pact-partner proxy flow, no multi-relay registration.

UnifiedPush and Web Push are post-launch additions when demand from de-Googled Android users and browser extension users materializes.

### 7. Content Moderation: Cut to Reports + Mutes

**Current complexity:** Kind 10064 reports with encrypted payloads, NIP-32 label integration with trust model (4-tier: direct follow, WoT proximity, moderation service subscription, community endorsement), blocklist federation (kind 10070), CSAM handling (hash matching with PhotoDNA/CSAI/PDQ, relay-level scanning, client-side defense-in-depth, emergency reporting path), coordinated harassment defense analysis, moderation services as specialized Nostr identities, pact-level moderation with common carrier principle, comparison with 4 other platforms.

**Proposed simplification:** Two features:

1. **Kind 10064 content reports.** User reports content to relay operators. Encrypted to relay operator pubkey. Done.
2. **NIP-51 mute lists.** Already exists in Nostr. User mutes pubkeys locally.

Cut the CSAM hash-matching pipeline (relay operators handle this per their jurisdiction, as they do today on Nostr -- it does not need protocol-level specification). Cut moderation services (no moderation services exist yet). Cut blocklist federation (see "Components to Cut" above). Cut the 5-platform comparison analysis.

### 8. Spam Resistance: Already Structural, Does Not Need a Document

**Current state:** A 270-line document analyzing spam resistance by device type (4 profiles), by attack type (7 attack categories), and by protocol phase (4 phases), with a summary table and 7 key properties.

**Proposed action:** This document is useful analysis but it is not a specification. Nothing in it needs to be implemented -- it describes emergent properties of the protocol. Keep it as internal reference material, but do not treat it as a design document that requires engineering effort. The spam resistance comes from WoT filtering and rate limiting, both of which are already specified in the whitepaper.

---

## Minimal Viable Protocol

The smallest version of Gozzip that delivers the core value proposition: "your data lives on your social graph, not on servers you do not control."

### Identity Layer

- **Root key** (secp256k1, Nostr-compatible)
- **Device delegation** (kind 10050) -- root key authorizes device subkeys via HKDF derivation
- **Governance key** -- signs profile and follow list updates
- **Social recovery** (kinds 10060, 10061) -- N-of-M threshold recovery with timelock. This is a pre-launch requirement (the whitepaper says so explicitly: "root key loss is permanent identity death" without it)

### Storage Layer

- **Checkpoints** (kind 10051) -- periodic Merkle root over events since last checkpoint
- **Storage pacts** (kind 10053) -- bilateral agreement to store each other's events, one type only (no media pacts)
- **Pact formation** (kinds 10055, 10056) -- request/offer/accept flow with WoT membership, volume matching (30% tolerance), 7-day account age
- **Challenge-response** (kind 10054) -- hash challenges only (no serve challenges, no byte-range challenges, no media challenges)
- **Target: 15 active pacts.** No equilibrium-seeking, no Poisson binomial, no PACT_FLOOR/CEILING. Form until 15. Replace on failure.
- **4-state pact FSM:** Forming, Active, Failing, Ended
- **Reliability scoring:** Exponential moving average, alpha=0.95. Below 70%: seek replacement. Below 50%: drop.

### Retrieval Layer

- **3-tier cascade:** Local (pact storage) -> Gossip (kind 10057 with TTL=3, rotating request tokens) -> Relay (standard Nostr query)
- **Read cache:** LRU, 100 MB default. Reads create replicas (cascading replication).
- **Data request/offer** (kinds 10057, 10058)

### Communication Layer

- **NIP-46 encrypted channels** for all peer-to-peer protocol messages
- **NIP-44 encrypted DMs** (kind 14)
- **Push notifications** (kind 10062) -- APNs + FCM, single notification relay, minimal filters

### Moderation Layer

- **Kind 10064 content reports** -- encrypted to relay operator
- **NIP-51 mute lists** -- existing Nostr infrastructure
- **Kind 10063 deletion requests** -- author-signed, best-effort propagation

### Media

- **Content-addressed references** -- `media` and `media_thumb` tags with SHA-256 hash
- **External hosting** -- CDN, S3, IPFS, self-hosted (user's choice)
- **Hash verification on fetch** -- any source serving correct bytes is acceptable
- No media pacts, no kind 10065, no media-specific challenges

### What Is NOT in the MVP

| Cut feature | Reason | When to add |
|------------|--------|-------------|
| BLE mesh / FIPS integration | Phase 4 feature, requires entirely separate transport stack | After v1 proves the pact model works |
| Media pacts | Separate pact economy doubles complexity; CDN hosting sufficient | When users demand peer-to-peer media sovereignty |
| Protocol versioning | Only one version exists | Before the first breaking change |
| Interplanetary compatibility | Not a real use case | Never (or decades from now) |
| Equilibrium-seeking formation | Over-engineered for launch; fixed target of 15 is sufficient | When telemetry shows pact count optimization matters |
| Standby pacts | Optimization that adds a state and promotion logic | When failover latency is a measured problem |
| Cached endpoint tier | Performance optimization | When gossip latency is a measured problem |
| Kind 10059 (endpoint hints) | Supports cached endpoint tier | Same as above |
| Kind 10065 (media manifest) | Supports media pacts | Same as above |
| Kind 10066 (guardianship completion) | Social signal, not functional | Post-launch |
| Kind 10067 (notification relay ad) | Discovery for a market that does not exist | When multiple notification relays exist |
| Kind 20062 (wake-up hint) | Optimization of push path | When push metadata exposure is a measured concern |
| Kind 10070 (blocklist federation) | Governance feature for mature network | When moderation communities exist |
| Blocklist federation | No blocklists exist yet | Post-launch |
| Moderation services | No moderation services exist yet | Post-launch |
| WoT visualization | Nice-to-have, not diagnostic | Post-launch |
| Protocol-level tracing | Developer tool | Post-launch |
| Relay compatibility scoring | Optimization | Post-launch |
| CSAM hash-matching pipeline | Relay operator concern, not protocol spec | Relay operator documentation |
| Quiet hours for push | UX polish | Post-launch |
| UnifiedPush / Web Push | <1% of users at launch | Post-launch |
| Guardian pacts | Bootstrap mechanism; can use Genesis Guardian nodes initially | When organic growth requires it |

### Event Kind Count: 19 to 11

**MVP event kinds:**

| Kind | Name | Purpose |
|------|------|---------|
| 10050 | Device Delegation | Device key authorization |
| 10051 | Checkpoint | Merkle root for reconciliation |
| 10052 | Conversation State | DM read-state |
| 10053 | Storage Pact | Reciprocal storage commitment |
| 10054 | Storage Challenge | Proof of storage |
| 10055 | Pact Request | Request storage partners |
| 10056 | Pact Offer | Respond to pact request |
| 10057 | Data Request | Pseudonymous retrieval |
| 10058 | Data Offer | Response with data |
| 10060 | Recovery Delegation | Social recovery contact |
| 10061 | Recovery Attestation | Recovery attestation |
| 10062 | Push Registration | Push token registration |
| 10063 | Deletion Request | GDPR-compatible deletion |
| 10064 | Content Report | Content/user report |

That is 14 kinds, down from 19. Kinds 10059 (endpoint hints), 10065 (media manifest), 10066 (guardianship completion), 10067 (notification relay ad), and 20062 (wake-up hint) are cut.

Of those 14, kinds 10063 and 10064 are simple fire-and-forget events. Kind 10052 is a local convenience event. The core protocol machinery is 11 kinds (10050-10058, 10060-10061) plus push registration (10062).

### Design Document Count: 7+ to 2

**MVP design documents needed:**

1. **Whitepaper** (trimmed) -- core protocol: identity, pacts, retrieval, gossip, WoT
2. **Protocol specification** (messages, pact state machine, challenge computation) -- implementer reference

Everything else is either analysis (spam resistance), future work (interplanetary, media pacts, protocol versioning), operational guidance (monitoring, push notification details), or legal (GDPR analysis) -- none of which is needed to build the first client.

### 12-Month Engineering Plan with 3 Engineers

| Quarter | Engineer 1 | Engineer 2 | Engineer 3 |
|---------|-----------|-----------|-----------|
| Q1 | Core identity: key hierarchy, device delegation (10050), social recovery (10060/10061) | Storage engine: local event store, checkpoint generation (10051), Merkle tree | Nostr client shell: relay connection, event publish/subscribe, NIP-65, basic UI |
| Q2 | Pact formation: request/offer/accept (10053/10055/10056), WoT computation, volume matching | Challenge-response (10054), reliability scoring, pact state machine (4 states) | Retrieval cascade: local lookup, gossip (10057/10058), relay fallback, read cache |
| Q3 | NIP-46 encrypted channels for pact communication, DM encryption (NIP-44), push notifications (10062) | Gossip forwarding: WoT-filtered forwarding, rate limiting, request deduplication, rotating tokens | Client UX: feed, DMs, pact health dashboard (simple), profile/follow management |
| Q4 | Integration testing, simulation validation against real pact behavior, bug fixes | Deletion (10063), content reports (10064), media tags (content-addressed references) | Beta testing, performance optimization, mobile (iOS/Android) polish |

This is aggressive but achievable. The key insight is that 60% of the current specification is not needed for v1.

---

## Summary of Verdict

| Component | Verdict | Action |
|-----------|---------|--------|
| Interplanetary | Cut entirely | Delete document, remove from whitepaper |
| Protocol versioning | Cut entirely (keep `protocol_version` tag) | Defer document until version 2 is needed |
| Media pacts | Cut entirely | Keep content-addressed references only |
| Blocklist federation | Cut entirely | Use existing NIP-51 mute lists |
| Kind 10066 (guardianship completion) | Cut | Guardian pacts expire via standard mechanism |
| Kind 10067 (notification relay ad) | Cut | Hardcode notification relay |
| Kind 20062 (wake-up hint) | Cut | Standard push flow sufficient |
| Kind 10059 (endpoint hints) | Cut | Merge into gossip tier |
| Kind 10065 (media manifest) | Cut | Media tags on events sufficient |
| Pact state machine | Simplify: 7 states to 4 | Forming/Active/Failing/Ended |
| Retrieval cascade | Simplify: 5 tiers to 3 | Local/Gossip/Relay |
| Pact formation model | Simplify: equilibrium-seeking to fixed target | Target 15 active pacts |
| Monitoring/diagnostics | Simplify to essentials | Pact list + simple health check |
| GDPR deletion | Simplify | Deletion request + existing challenge verification |
| Push notifications | Simplify | APNs + FCM only, single relay, minimal config |
| Content moderation | Simplify | Reports + mutes only |
| Spam resistance document | Reclassify | Analysis document, not a design spec |

The protocol's core insight -- bilateral storage pacts enforced by challenge-response within a WoT boundary -- is strong. The implementation surface obscuring that insight is the problem. Strip it back to the mechanism that matters, ship it, measure what actually fails, and add complexity only where measurement demands it.
