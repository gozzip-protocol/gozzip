# ADR 005: Storage Pact Layer

**Date:** 2026-02-28
**Status:** Accepted

## Context

The current architecture depends on relays for event storage and delivery. The design principles claim "users are not dependent on relays," but every data flow routes through relay infrastructure. If all relays a user publishes to disappear, their data is lost. This contradicts the sovereignty goal.

We need a model where users' data survives independent of any relay. The data must be retrievable even when the author's devices are offline.

## Decision

Introduce a **storage pact layer** — reciprocal storage commitments between volume-matched peers in each other's web of trust.

### Core properties

- **Reciprocal** — "I store yours, you store mine." If one side stops, the other stops.
- **Volume-balanced** — peers are matched by data volume so risk is symmetric.
- **Private** — pact details are exchanged directly, never published. Network topology is hidden.
- **Time-bounded** — each pact covers events from the latest checkpoint forward (~monthly window).
- **DVM-style discovery** — users broadcast pact requests, qualifying peers respond.
- **Challenge-response proof** — periodic random challenges verify peers actually hold the data.
- **DVM-based retrieval** — when someone wants your events, your storage peers respond to data requests without revealing the pact topology.

### Why not a DHT?

DHTs (Kademlia, IPFS) store strangers' data with no social incentive. Lookup latency is high. The WoT-based reciprocal model creates natural incentives — you get backup storage by providing it — and leverages the existing follow-as-commitment principle.

### Why not just more relays?

More relays still means dependency on third parties. The storage pact model makes users each other's infrastructure. Relays can coexist as accelerators but are not required.

## Consequences

- Users' data survives relay disappearance — storage peers hold recent events
- Natural incentive structure — storage is reciprocal, not altruistic
- Private topology — censors can't target storage peers because pacts are hidden
- Follow-as-commitment extends to storage — your WoT IS your infrastructure
- Adds 6 new event kinds (10053–10058) for pact management and retrieval
- Mobile devices participate when possible but aren't obligated as always-on servers
- Relays become optional accelerators, not required infrastructure
- Gradual migration — relays still work alongside storage peers
