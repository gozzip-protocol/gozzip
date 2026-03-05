# User

A sovereign participant in the Gozzip network. Identified by a root keypair, operates through one or more device subkeys. Not dependent on relays — responsible for their own social graph.

## Personas

A user may hold one or more of these named roles depending on their node type, network maturity, and voluntary commitments. See [Glossary](../glossary.md).

| Persona | Role | Description |
|---------|------|-------------|
| **Keeper** | Full node pact partner | Stores complete event history for pact partners. Always-on (95% uptime). |
| **Witness** | Light node pact partner | Stores recent events (~monthly) for pact partners. Phase 1: browser extension/web (60% uptime). Phase 2: mobile (30% uptime). |
| **Guardian** | Bootstrap sponsor | Volunteers storage for one untrusted newcomer outside their WoT. |
| **Seedling** | Bootstrapping newcomer | Growing into the network, receiving initial storage support. |
| **Herald** | Relay operator | Curates and relays content for beyond-graph reach. |

A single user can hold multiple persona roles simultaneously — for example, a *Keeper* who also acts as a *Guardian*.

## Responsibilities

- Maintain their root keypair and device delegations
- Manage key hierarchy — root key in cold storage, derived keys on devices
- Choose which devices are DM-capable (`dm` flag in kind 10050) — controls which devices hold the DM private key
- Designate recovery contacts — N-of-M social recovery via kind 10060 for root key loss. See [ADR 008](../decisions/008-protocol-hardening.md).
- Curate their follow list — following someone is a commitment to index that person's network
- Publish events (posts, reactions, DMs, etc.) signed by device subkeys
- Publish checkpoints (kind 10051) to enable light node sync
- Optionally run a full node to maintain complete history of their follows

## Follow-as-Commitment

Following someone is not free. When you follow a user, you commit to indexing their network:

- Your node tracks their events and the events of their connections
- This gives you rapid access to what matters to you
- You don't store all information of every user — checkpoints and light node sync keep it efficient
- Discovery beyond your immediate follows is still possible through the web of trust, but it's not instant

This changes follow behavior: you keep your follows wisely because you bear the indexing cost.

## Storage Obligations

Every user commits to storing recent events for 10–40+ volume-matched WoT peers (scaled by follower count, ~20 typical). This is the reciprocal storage pact model — see [Storage](../architecture/storage.md).

- You store their events since their last checkpoint (~monthly)
- They store yours in return
- Challenge-response (kind 10054) verifies you hold the data
- If you stop storing, they stop storing yours — natural consequence
- Your storage peers serve your events when your devices are offline

Storage obligations scale with pact count: bootstrap phase (0–5 pacts, relay-primary), hybrid phase (5–15, mixed), sovereign phase (15+, peer-primary).

## Incentives

Contributing to the network earns wider content reach — no tokens, no subscriptions. See [ADR 009](../decisions/009-incentive-model.md).

- **Storage reliability → reach** — your pact partners prioritize forwarding your content through gossip. More reliable storage = more pact partners = more forwarding advocates = wider distribution.
- **Lightning boost** — optionally pay relays via zaps for priority delivery, extended retention, or content boost. Transparent and competitive.
- **No cliff** — less contribution means less reach, not exclusion. The network works for everyone, just smaller for passive users.

## Offline Resilience

Gozzip works without internet through BLE mesh and local queuing. Inspired by [bitchat](https://github.com/permissionlesstech/bitchat). See [ADR 010](../decisions/010-bitchat-integration.md).

- **BLE mesh** — nearby devices exchange events via Bluetooth. Multi-hop relay up to 7 hops. Interoperable with bitchat's mesh network. No internet required.
- **Store-and-forward** — events are queued locally when no transport is available. Auto-delivered when connectivity returns.
- **Nearby mode** — discover nearby users via ephemeral subkeys and geohash tags. Identity is never revealed unless the user explicitly chooses to. Activates/deactivates on demand.
- **Panic wipe** — emergency destruction of all local data (keys, events, state). Recovery via cold storage root key and storage peers. Optional decoy mode.

## Light Node vs Full Node

| | Light node (*Witness*) | Full node (*Keeper*) |
|---|---|---|
| Sync method | Checkpoint (kind 10051) + last N events per device | Full event history |
| Storage | Minimal — only recent state | Complete — all follows' history |
| Offline tolerance | Needs to catch up from checkpoint | Already has everything |
| Who runs it | Mobile clients, casual users | Power users, archivists |
| Storage pacts | Participates when online — stores only checkpoint window (~monthly) for pact partners | Full participation — always-on, stores complete history for pact partners |
| Expected uptime | 30% | 95% |

## Interactions

- **Relay** — publishes events to relays, subscribes to feeds
- **Other users** — follows, DMs, reactions, reposts, zaps
- **Oracle** — resolution of device → root identity happens transparently
- **Bot** — may interact with bots for automated services
- **Bridge** — events from followed users on other networks arrive via bridges
- **Storage peers** — reciprocal data storage via pacts, gossip discovery via kind 10059 endpoint hints
- **Seedling** (as Guardian) — stores one newcomer's data voluntarily via guardian pact

## Resolved Questions

- **What's the default light node sync depth?** — 50 events per device. Covers roughly 2 days of activity for an active user. Combined with the checkpoint, this gives light nodes a recent window without excessive bandwidth.
- **How does a new follower bootstrap?** — From the follow point forward. Historical events are available on request (via storage peers or relays) but not pushed automatically. This aligns with follow-as-commitment — you commit to indexing from now, not retroactively.
- **Can users set per-follow indexing depth?** — Yes. Clients can implement tiered indexing — deep for close contacts (full history), shallow for acquaintances (last checkpoint only). This is client-side logic, not a protocol concern.
