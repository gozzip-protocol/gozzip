# Glossary

Terminology, personas, and protocol concepts used throughout the Gozzip documentation.

## Personas

Named roles that participants can hold in the network. A single user may hold multiple persona roles simultaneously.

| Persona | Protocol Role | Human Parallel | Description | Expected Uptime |
|---------|--------------|----------------|-------------|-----------------|
| **Keeper** | Full node pact partner | Inner circle (5–15) | Stores complete event history for pact partners. Always-on. | 95% |
| **Witness** | Light node pact partner | Extended circle (50–150) | Stores recent events (~monthly window) for pact partners. | 60% (extension/web) |
| **Guardian** | Bootstrap sponsor | Community patron | Volunteers storage for one untrusted newcomer outside their WoT. | — |
| **Seedling** | Bootstrapping newcomer | New community member | Growing into the network, receiving initial storage support. | — |
| **Herald** | Relay operator | Town crier | Curates and relays content for beyond-graph reach. | — |

## Protocol Terms

**Storage pact** — A bilateral agreement between two nodes to store each other's events. Reciprocal, volume-balanced within 30% tolerance. Formalized as kind 10053.

**Bootstrap pact** — A one-sided temporary pact triggered when a new user follows someone. The followed user stores the newcomer's data. Auto-expires after 90 days or 10 reciprocal pacts. No WoT required.

**Guardian pact** — A voluntary one-sided pact where an established user (*Guardian*) stores data for one untrusted newcomer (*Seedling*) outside their WoT. One slot per Guardian. Expires after 90 days or when the Seedling reaches Hybrid phase (5+ pacts). Kind 10053 with `type: guardian` tag.

**Archival pact** — A pact covering full history or a deep range, with lower challenge frequency (weekly). For power users and archivists.

**Standby pact** — A pact where the partner receives events but is not actively challenged. Promoted to active when an active partner fails, providing instant failover.

**Active pact** — A pact where the partner is regularly challenged and expected to serve data on request. Default: 20 per user.

**Web of Trust (WoT)** — The graph of follow relationships. WoT distance *d(p,q)* is the minimum follow-hops between two nodes. The protocol operates within a 2-hop boundary.

**Inner Circle** — Feed tier 1. Mutual follows whose content is continuously synced via storage pacts. Always available, 30-day cache (pact window).

**Orbit** — Feed tier 2. Authors the user interacts with heavily, plus authors socially endorsed by 3+ Inner Circle contacts. Polled periodically, 14-day cache TTL.

**Horizon** — Feed tier 3. 2-hop graph reach weighted by edge count, plus relay-curated discoveries. On-demand fetching, 3-day cache TTL.

**Interaction score** — A per-author rolling score computed from public interactions (replies weight 3, reposts weight 2, reactions weight 1) with exponential recency decay (half-life ~21 days). Used for feed ranking and referral signals.

**Referral threshold** — The point at which an author enters a user's Orbit: 3+ Inner Circle contacts with interaction scores above the minimum (default: 5 weighted interactions in 30 days).

**Checkpoint** — A periodic reconciliation marker (kind 10051) containing per-device event heads, a Merkle root, and profile references. Defines the storage obligation boundary for light pact partners.

**Pact-aware gossip routing** — Gossip forwarding prioritized by social proximity: active pact partners first, then 1-hop WoT, then 2-hop WoT. Unknown pubkeys are never forwarded.

**Rotating request token** — A daily-rotating pseudonymous lookup key `H(target_pubkey || YYYY-MM-DD)` used in data requests (kind 10057). Prevents casual cross-day request linkage but is reversible by any party that knows the target's public key. Not a formal cryptographic blinding scheme.

**Data availability verification** — The challenge-response mechanism used to verify that pact partners can produce stored data on demand. Proves accessibility within the response window, not persistent local storage.

**Three-phase adoption** — Client behavior adapts based on pact count: Bootstrap (0–5 pacts, relay-primary), Hybrid (5–15, mixed), Sovereign (15+, peer-primary).

**Cascading read-cache** — When a node fetches events, it holds a local copy and can serve subsequent gossip requests. Popular content naturally replicates across the follower base. Bounded by configurable cache size (default 100 MB).

**Reliability score** — A per-peer rolling score using exponential moving average (alpha = 0.95) over 30-day challenge-response results. Healthy >= 90%, degraded 70–90%, unreliable 50–70%, failed < 50%.

## Delivery Tiers

Event retrieval follows a cascade of increasingly expensive paths:

| Tier | Path | Description |
|------|------|-------------|
| 0 | **BLE mesh** | Nearby devices serve events via Bluetooth Low Energy. No internet required. |
| 1 | **Cached endpoints** | Direct connection to known storage peer endpoints (kind 10059). Zero broadcast overhead. |
| 2 | **Gossip** | Pseudonymous data request via rotating request token (kind 10057) broadcast to WoT peers, TTL=3. |
| 3 | **Storage peers via DVM** | Traditional kind 10057 broadcast through relay infrastructure. |
| 4 | **Relay fallback** | Traditional relay query as last resort. |

## Node Types

| Type | Persona | Storage Depth | Expected Uptime |
|------|---------|--------------|-----------------|
| **Full** | Keeper | Complete event history | 95% |
| **Light** | Witness | Rolling checkpoint window (~monthly) | 60% (extension/web) |
