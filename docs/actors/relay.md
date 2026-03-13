# Relay

A server that stores and forwards events. **Standard Nostr relays work with Gozzip without any code changes.** All protocol intelligence lives in clients — the relay is a dumb pipe with storage.

In the persona model, relay operators are *Heralds* — town criers who curate and relay content for beyond-graph reach. See [Glossary](../glossary.md).

## Standard Relay Compatibility

Gozzip runs on unmodified Nostr relays (strfry, nostream, relay.tools, etc.). Every Gozzip event kind (10050–10061) is a valid Nostr event. Relays store them, serve them on subscription, and forward them to connected clients — exactly what they already do.

### What standard relays provide

- **Event storage** — all Gozzip kinds are stored like any Nostr event
- **Subscription delivery** — `REQ` with filters works for all Gozzip events
- **Mobile "mailbox"** — relays already persist events. When Phone B reconnects, it queries `since: <last_timestamp>` and gets everything missed. No special buffering needed.
- **NIP-65 relay lists** — clients track which relays each user publishes to, enabling discovery and failover
- **Retention policies** — relay-defined. Since storage peers hold canonical data, relay retention doesn't affect data durability.

### What clients handle (not relays)

| Capability | Who does it | How |
|-----------|------------|-----|
| Gossip forwarding (TTL) | Clients | Client receives Kind 10057, can't respond, decrements TTL, publishes to own relays |
| Rotating request token matching | Storage peers (clients) | Each storage peer computes `H(stored_pubkey \|\| date)` and matches incoming requests |
| WoT-only forwarding | Clients | Client checks source pubkey against local WoT graph before forwarding |
| Rate limiting / dedup | Clients | Client enforces per-source rate limits and `request_id` LRU cache |
| Device→root resolution | Clients | Client fetches Kind 10050, extracts device pubkeys, queries each separately |
| Endpoint hint privacy | Clients | Kind 10059 wrapped in NIP-59 gift wrap — relay stores opaque blob, only recipient decrypts |
| Push notifications | Separate service | Notepush pattern — service subscribes to relay, sends APNS/FCM wake-ups |

### Relay reliability and failover

Clients track relay health per NIP-65:

1. Publish event to relay
2. Query back to verify storage
3. If relay dropped it → publish to next relay from NIP-65 list
4. If relay consistently drops events → remove from NIP-65, switch relay
5. Checkpoint reconciliation (Kind 10051) catches any gaps regardless

Users connect to 3–5 relays. If one goes down, others continue. No single relay is critical.

---

## Gozzip-Optimized Relay (Optional Accelerators)

> **These optimizations are NOT required.** The protocol works 100% on standard relays. Optimized relays make things faster, not possible. These are the features we propose to the Nostr relay ecosystem as improvements that benefit all users.

### Oracle Resolution

When a client subscribes to `root_pubkey`, a standard relay requires the client to:
1. Fetch Kind 10050 → get device pubkeys
2. Query each device pubkey separately
3. Merge results client-side (3+ round-trips)

An optimized relay can:
1. Cache Kind 10050 device→root mappings
2. Index events by `root_identity` tag
3. Return a unified feed for a root pubkey in one round-trip

**Impact:** Saves 2 round-trips per subscription. Significant for mobile bandwidth.

### Checkpoint Delta

Standard relay returns all matching events when queried. An optimized relay can:
1. Accept a Kind 10051 checkpoint reference in the subscription
2. Return only events AFTER each device's head sequence
3. Skip events the client already has

**Impact:** Saves bandwidth for light nodes. Instead of fetching thousands of events, client gets the ~20-50 new ones.

### Light Node Shortcut

Fetch Kind 10051 (checkpoint) → get device heads → serve only events after head per device. Reduces sync payload from full history to a delta.

### Gossip Relay Forwarding

Standard relays store events and serve subscribers. An optimized relay can also:
1. Detect Kind 10055/10057 with `ttl` > 0
2. Decrement TTL
3. Forward to peer relays (relay-to-relay sync)
4. Apply rate limits (10 req/s for 10055, 50 req/s for 10057) and request dedup

**Impact:** Gossip reaches more nodes faster. Without this, gossip still propagates through client-side forwarding — just slower.

### `root_identity` Tag Validation

Verify the `root_identity` tag against Kind 10050 before indexing. Prevents impersonation where a rogue client claims a `root_identity` it's not authorized to use. Cost is one 10050 lookup per new device pubkey, cached after first verification.

### NAT Hole Punching (Optional)

Relays can help pact partners establish direct connections by acting as a signaling server for NAT traversal. This **reduces** relay load — after a successful hole punch, pact traffic flows directly between peers instead of through the relay.

**How it works:**

```
Alice (desktop, behind NAT) ──WS──► Relay ◄──WS── Bob (desktop, behind NAT)
                                     │
                              Relay knows both public IP:ports
                              (visible from TCP connections)
                                     │
                    ┌────────────────┴────────────────┐
                    ▼                                  ▼
           Alice receives                     Bob receives
           Bob's public IP:port               Alice's public IP:port
                    │                                  │
                    └──── UDP packets to each other ───┘
                              NAT tables punched
                              Direct channel open
```

**Authenticated handshake (required before any data flows):**

After the UDP channel opens, both sides must prove identity before exchanging pact data. This prevents MITM attacks where an attacker intercepts the hole punch:

1. Alice sends a challenge: `{nonce: random_32_bytes, device_pubkey: alice_device_pub}`
2. Bob verifies `alice_device_pub` is listed in Alice's Kind 10050 (cached from pact formation)
3. Bob responds: `{nonce_response: sign(alice_nonce, bob_device_key), nonce: random_32_bytes, device_pubkey: bob_device_pub}`
4. Alice verifies Bob's signature against `bob_device_pub` from Bob's Kind 10050
5. Alice responds: `{nonce_response: sign(bob_nonce, alice_device_key)}`
6. Bob verifies Alice's signature
7. Mutual authentication complete — pact traffic flows over the direct channel

Both sides reject the connection if any signature fails or if the device pubkey isn't in the expected Kind 10050. The channel is then encrypted using the exchanged device keys (NIP-44 or Noise NK pattern).

**Overhead for the relay:**

| Cost | Amount |
|------|--------|
| Memory | ~100 bytes per connected client (pubkey → public IP:port mapping) |
| CPU | Negligible — exchange two addresses per request |
| Bandwidth | ~500 bytes per hole-punch coordination |
| **Net effect** | **Negative** — relay offloads data traffic after successful punch |

**Success rate by pair type:**

| Pair | Success rate | Worth attempting? |
|------|-------------|-------------------|
| Full ↔ Full | ~85-90% | **Yes** — persistent direct connection, major relay offload |
| Full ↔ Active | ~80-85% | **Yes** — direct while both online |
| Active ↔ Active | ~80% | **Yes** — direct when both browsers/apps open |
| Full ↔ Intermittent | ~75% | **Maybe** — works when phone is active, breaks on WiFi→cellular |
| Active ↔ Intermittent | ~70% | **Marginal** — short overlap, fragile |
| Intermittent ↔ Intermittent | ~70% | **No** — short overlap, ~20 KB/day doesn't justify setup |

Symmetric NAT (where hole punching always fails) falls back to relay-mediated communication — which is exactly what's already working. No degradation.

**Why relays benefit from offering this:** Every successful hole punch removes a pair's ongoing traffic from the relay. For infrastructure relays serving thousands of users, this is significant bandwidth savings. The relay invests 500 bytes of signaling and offloads potentially megabytes of daily pact traffic.

---

## Relay Types

Not all relays serve the same purpose. Different types provide different value to the network.

All three relay types below are *Herald* variants — they differ in curation strategy but share the Herald role of extending reach beyond the social graph.

**Discovery relays** — curate content by topic, quality, or community. They index selectively, surfacing posts they think are valuable. Users follow a discovery relay's feed like they follow a person. The relay's value is taste — it earns followers by being a good filter.

**Infrastructure relays** — store broadly, serve fast, high availability. Their value is performance and reliability. They remain useful as accelerators even when users have 15+ storage pacts (sovereign phase).

**Community relays** — serve specific groups (NIP-29 group chats, local communities, interest groups). Their value is exclusivity and curation within a community. Membership in the community is the incentive to use the relay.

## Relay-as-Curator

A relay has its own root pubkey. It publishes curated event lists, recommendations, or indexes. Users who find value subscribe (follow the relay). The relay's follower count and WoT position determine how far its curations travel through gossip. See [ADR 009](../decisions/009-incentive-model.md).

- Good curation → more followers → wider reach → more users want to publish through the relay
- Publishing through a well-connected relay makes your content visible to that relay's subscribers — an audience beyond your own WoT
- The relay becomes a distribution channel — users who contribute good content to the relay's community are more likely to be curated

## Lightning Services

Relays can offer premium services activated via zaps (kind 9734/9735). This is the optional Lightning layer on top of the free attention economy. See [ADR 009](../decisions/009-incentive-model.md).

| Service | What you get | How it works |
|---------|-------------|--------------|
| Priority delivery | Faster indexing, wider push to relay subscribers | Zap relay per-event or per-month |
| Extended retention | Events kept beyond default window | Zap relay for duration (e.g., 100 sats/month for 1-year retention) |
| Content boost | Specific post featured in relay's curated feed | Zap relay with tagged zap pointing to the event. Marked as boosted (transparent). |
| Onion routing | Gossip requests wrapped in NIP-44 encrypted layers per hop — hides request path from intermediate nodes | Zap relay for onion routing access. Primarily useful for desktop/web clients. |
| Relay-defined services | Whatever the operator invents | Analytics, custom filtering, API access, webhooks — relays compete on features |

**Properties:**
- **Transparent** — boosted content is visibly marked. No dark patterns.
- **Competitive** — different relays offer different prices and services. Market dynamics, not protocol-mandated pricing.
- **Optional** — the free attention layer works without any Lightning.
- **Accountability** — relays that take sats but deliver poorly lose subscribers to competitors.

## Interactions

- **User/Client** — accepts published events, serves subscription feeds
- **Other relays** — may replicate events (relay-to-relay sync)
- **Bridge** — forwards events to/from other networks

## Resolved Questions

- **What are the incentives for running a relay?** — Ecosystem-native value through the three-layer incentive model. Free layer: relay-as-curator earns followers → influence → users publish through it. Lightning layer: priority delivery, extended retention, content boost, and relay-defined services paid via zaps. Different relay types (discovery, infrastructure, community) monetize their unique value differently. No external subscriptions needed — though operators can still charge if they want. See [ADR 009](../decisions/009-incentive-model.md).
- **Do relays need to be modified for Gozzip?** — No. Standard Nostr relays work without changes. All protocol intelligence (gossip forwarding, rotating request token matching, WoT filtering, device resolution) lives in clients. Optimized relays can accelerate performance but are never required.
- **How much oracle logic belongs in the relay vs in a separate service?** — Oracle resolution (device → root) is a relay optimization, not a requirement. Clients can resolve it themselves by fetching Kind 10050. An optimized relay caches the mapping and indexes by `root_identity` tag for faster lookups.
- **Relay retention policies — how long are events kept?** — Relay-defined. Since storage peers hold canonical data, relays can set aggressive retention (e.g., 30 days) without risk of data loss. Always-on relays may keep longer for discoverability. No protocol-level retention requirement.
- **Should relays validate `root_identity` tags against kind 10050, or trust clients?** — Optional optimization. An optimized relay validates to prevent impersonation. Standard relays ignore the tag — clients verify authorship themselves via Kind 10050. Both paths work correctly.
