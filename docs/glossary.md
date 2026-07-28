# Glossary

Terminology, personas, and protocol concepts used throughout the Gozzip documentation.

## Personas

Named roles that participants can hold in the network. A single user may hold multiple persona roles simultaneously.

| Persona | Protocol Role | Human Parallel | Description | Expected Uptime |
|---------|--------------|----------------|-------------|-----------------|
| **Keeper** | Full node pact partner | The friends who remember everything (Dunbar 5–15) | Stores complete event history for pact partners. Always-on. | 95% |
| **Witness** | Light node pact partner | Extended circle (50–150) | Stores recent events (~monthly window) for pact partners. | 60% (Phase-1 extension/web), 30% (Phase-2 mobile) |
| **Guardian** | Bootstrap sponsor | Community patron | Volunteers storage for one untrusted newcomer outside their WoT. Eligible at 15+ pacts (relaxed to 5+ during the genesis period — see [genesis-bootstrap](design/genesis-bootstrap.md)). | — |
| **Seedling** | Bootstrapping newcomer | New community member | Growing into the network, receiving initial storage support. | — |
| **Herald** | Relay operator | Town crier | Curates and relays content for beyond-graph reach. A permanent role — relays are reduced, not optional. | — |

> "Inner Circle" (capitalized) refers specifically to feed **tier 1** (see below). A Keeper is a full-node pact partner; the two often overlap but are not the same set.

## Protocol Terms

**Storage pact** — A bilateral agreement between two nodes to store each other's events. Reciprocal but capped-asymmetric: each partner stores whatever the other produces, up to ~10 MB/month per partner (no volume matching — see [ADR 013](decisions/013-capped-asymmetric-pacts.md)). Formalized as kind 10053. A healthy pact renews by default at 90 days ([ADR 015](decisions/015-renewal-by-default-pacts.md)).

**Bootstrap pact** — A one-sided temporary pact triggered when a new user follows someone. The followed user stores the newcomer's data. Auto-expires after 90 days or 10 reciprocal pacts. No WoT required.

**Guardian pact** — A voluntary one-sided pact where an established user (*Guardian*) stores data for one untrusted newcomer (*Seedling*) outside their WoT. One slot per Guardian. Expires after 90 days or when the Seedling reaches Hybrid phase (5+ pacts) — the newcomer has grown roots. Kind 10053 with `type: guardian` tag.

**Active pact** — A pact regularly challenged and expected to serve data on request. Target: ~20 active per user, floor 12 ([ADR 016](decisions/016-target-based-pact-formation.md)). *(Standby and archival pacts are deferred post-v1 — see [docs/future](future/README.md).)*

**Web of Trust (WoT)** — The graph of follow relationships. WoT distance *d(p,q)* is the minimum follow-hops between two nodes. The protocol operates within a 2-hop boundary.

**Inner Circle** — Feed tier 1. Mutual follows whose content is continuously synced via storage pacts. Always available, 30-day cache (pact window).

**Orbit** — Feed tier 2. Authors the user interacts with heavily, plus authors socially endorsed by 3+ Inner Circle contacts. Polled periodically, 14-day cache TTL.

**Horizon** — Feed tier 3. 2-hop graph reach weighted by edge count, plus relay-curated discoveries. On-demand fetching, 3-day cache TTL.

**Interaction score** — A per-author rolling score computed from public interactions (replies weight 3, reposts weight 2, reactions weight 1) with exponential recency decay (half-life ~21 days). Used for feed ranking and referral signals.

**Referral threshold** — The point at which an author enters a user's Orbit: 3+ Inner Circle contacts with interaction scores above the minimum (default: 5 weighted interactions in 30 days).

**Checkpoint** — A periodic reconciliation marker (kind 10051) containing per-device event heads, a Merkle root, and profile references. Defines the storage obligation boundary for light pact partners.

**Pact-aware gossip routing** — Gossip forwarding prioritized by social proximity: active pact partners first, then 1-hop WoT, then 2-hop WoT. Unknown pubkeys are never forwarded. *(Gossip is an optional censorship-resistance extension in v1, not a required retrieval tier — see [ADR 017](decisions/017-three-tier-retrieval.md).)*

**Data request** — A NIP-44-encrypted direct request (kind 10057) to a chosen storage peer, addressed over iroh. Reader privacy relative to Nostr comes from *where* metadata flows (WoT peers, not arbitrary relays), not from requester anonymity ([ADR 017](decisions/017-three-tier-retrieval.md)).

**Data availability verification** — The challenge-response mechanism used to verify that pact partners can produce stored data on demand (kind 10054). Proves accessibility within the response window, not persistent local storage.

**Three-phase adoption** — Client behavior adapts based on pact count: Bootstrap (0–5 pacts, relay-primary), Hybrid (5–15, mixed), Sovereign (15+, peer-primary).

**Cascading read-cache** — When a node fetches events, it holds a local copy and can serve subsequent gossip requests. Popular content naturally replicates across the follower base. Bounded by configurable cache size (default 100 MB).

**Reliability score** — A per-peer, *presence-aware* score: partners are challenged only while observed online, and the score is `passes ÷ challenges-issued-while-online` over a rolling 14-day window ([ADR 014](decisions/014-presence-aware-reliability.md)). Bands: Healthy ≥90%, Degraded 70–90%, Unreliable 50–70%, Failed <50%. A partner is dropped only when Failed is sustained ≥14 days. Presence-awareness is what lets a 30%-uptime Witness score as Healthy instead of as a defector.

**DVM** — Data Vending Machine (NIP-90): a request/response service pattern on Nostr, used here for relay-mediated storage-peer lookups as a fallback.

## Retrieval Tiers

Reading an event follows a three-tier cascade ([ADR 017](decisions/017-three-tier-retrieval.md)):

| Tier | Path | Description |
|------|------|-------------|
| 1 | **Local** | The reader's own pact storage and read-cache. The common case. |
| 2 | **Partner query** | A direct, NIP-44-encrypted request (kind 10057) to the author's known pact partners, addressed over iroh. |
| 3 | **Relay** | Fallback query through relay infrastructure, as in Nostr. |

Gossip (WoT-filtered forwarding) and BLE mesh are optional/deferred extensions, **not** required tiers.

## Node Types

| Type | Persona | Storage Depth | Expected Uptime |
|------|---------|--------------|-----------------|
| **Full** | Keeper | Complete event history | 95% |
| **Light** | Witness | Rolling checkpoint window (~monthly) | 60% (Phase-1 extension/web), 30% (Phase-2 mobile) |
