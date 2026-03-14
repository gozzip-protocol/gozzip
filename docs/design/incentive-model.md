# Incentive Model — Design Document

**Date:** 2026-03-01
**Status:** Approved

## Principle

Value comes from within the ecosystem. Attention and reach are the primary currency — contributing more to the network makes your content more discoverable. Lightning (zaps) provides an optional premium layer on top.

No external subscriptions. No tokens. No mandatory payments.

## Pact-Aware Gossip Routing

The core mechanism. When a node forwards gossip or decides what content to propagate, it applies priority ordering:

1. **Active pact partners** — highest priority. You store their data, you forward their content eagerly. Self-interested: if their content is discoverable, your storage pact has value.
2. **WoT contacts** (1-hop follows) — standard priority.
3. **Extended WoT** (2-hop) — lower priority, forwarded if capacity allows.
4. **Unknown pubkeys** — served locally if available, never forwarded.

A user with 20 reliable pact partners has 20 nodes that eagerly forward their content. A user with 5 flaky pacts has fewer advocates. No score published — the network topology IS the incentive.

When a pact drops due to low reliability, the dropped peer loses a forwarding advocate. Their content distribution naturally shrinks.

**Simulation evidence (pact churn).** 30-day simulations across multiple topologies show that pact churn is net-negative in ALL tested configurations — more pacts dissolve than form over the simulation period. Churn rates range from 2.79 pact changes/node/day in sparse graphs (Barabasi-Albert m=10) to 6.87/node/day in dense graphs (BA m=50), rising to 8.04/node/day when timezone-correlated availability is applied. This means the "20 reliable pact partners" scenario described above is harder to sustain than expected: the network contracts rather than stabilizes under realistic conditions. Sparse hub-and-spoke topologies (BA m=10) produce significantly lower churn, suggesting that well-connected hubs act as pact anchors — nodes paired with high-degree hubs maintain more stable relationships.

## Relay Types and Relay-as-Curator

Relays aren't one thing. Different types provide different value.

**Discovery relays** — curate content by topic, quality, or community. Users follow a discovery relay's feed like they follow a person. The relay earns followers by being a good filter.

**Infrastructure relays** — store broadly, serve fast, high availability. Their value is performance and reliability. They remain useful as accelerators even in the sovereign phase (15+ pacts).

**Community relays** — serve specific groups (NIP-29 group chats, local communities, interest groups). Their value is exclusivity and curation within a community.

**How relays earn reach:** A relay has its own root pubkey. It publishes curated event lists, recommendations, or indexes. Users who find value subscribe (follow). The relay's follower count and WoT position determine how far its curations travel through gossip.

**How users benefit from relays:** Publishing through a well-connected relay makes your content visible to that relay's subscribers — an audience beyond your own WoT. The relay is a distribution channel.

## The Contribution-Reach Feedback Loop

**For a regular user:**
1. Join, follow people → bootstrap pacts form
2. Store reliably for pact partners → they forward your content eagerly
3. Posts reach more people through gossip → gain followers
4. More followers = more potential pact partners = more forwarding advocates
5. Content gets picked up by discovery relays → even wider reach

**For a relay operator:**
1. Run a relay, curate good content → users subscribe
2. More subscribers → curations travel further through gossip
3. Users want to publish through you → more content to curate
4. Relay becomes a hub for discovery beyond individual WoT graphs

**For a passive user (lurker / read-only):**
1. Lurkers (60-80% of users on typical social platforms) have no content to offer in bilateral pacts
2. The reach reward (more forwarding) has zero value for users who don't produce content
3. Rational behavior for lurkers is to form zero pacts — defection is the dominant strategy
4. Lurkers consume content through relays and gossip read-caches from followed authors' pact partners
5. This is an honest equilibrium: lurkers are relay-dependent consumers, not sovereign participants
6. The protocol's incentive model primarily serves the 10-20% of users who actively create content

A "consumer mode" (relay-dependent, no pacts) should be a first-class client experience for passive users, not a degraded state. The protocol's data sovereignty claims apply to content creators, not lurkers.

**No individual cliff.** There's no minimum contribution to participate. Less contribution means less reach, not exclusion. However, the cooperative equilibrium is fragile at the network level: game-theoretic analysis shows that above approximately 30% free-riding, the incentive to cooperate degrades for remaining honest nodes, and above 70%, data availability becomes unreliable. The network has two stable equilibria — universal cooperation (everyone stores) and universal defection (nobody stores, relay fallback only). The protocol's goal is to maintain the cooperative equilibrium through the reach gradient, but this is not guaranteed.

**Simulation evidence (cooperative equilibrium fragility).** Simulations demonstrate net pact contraction even with zero strategic defectors — all nodes behave cooperatively, yet the network still sheds pacts over 30 days. This suggests the 30% defection tipping point identified in game-theoretic analysis may be lower than expected in practice, because the volume-tolerance threshold (delta=0.30) and reliability scoring already cause honest nodes to fail pact maintenance under realistic activity variance and availability patterns. The cooperative equilibrium may be unstable not just due to free-riding, but due to the protocol's own scoring mechanisms being too aggressive for heterogeneous node populations.

**Keeper ratio assumption:** The plausibility analysis models 25% of users running always-on full nodes (Keepers). This is an optimistic target. Comparable systems achieve 0.1-5% always-on participation (Bitcoin full nodes: ~0.01%, Mastodon instance operators: ~2%, Nostr relays: 0.2-1%). The protocol is designed to function at full-node ratios as low as 5%. At 5% Keepers, the all-light-node availability analysis applies (P(unavailable) ≈ 0.08%), which remains acceptable. The incentive loop above is designed to encourage Keeper operation organically, but the protocol must not depend on achieving 25%.

## Lightning as the Premium Layer

The base layer is free. Lightning adds a premium tier — relays publish a service menu, users zap to activate services.

| Service | What you get | How it works |
|---------|-------------|--------------|
| Priority delivery | Faster indexing, wider push to relay subscribers | Zap relay per-event or per-month |
| Extended retention | Events kept beyond default window | Zap relay for duration (e.g., 100 sats/month for 1-year retention) |
| Content boost | Specific post featured in relay's curated feed | Zap relay with tagged zap pointing to the event. Marked as boosted (transparent). |
| Relay-defined services | Whatever the operator invents | Analytics, custom filtering, API access, webhooks — relays compete on features |

**Properties:**
- **Transparent** — boosted content is visibly marked
- **Relay-competitive** — different relays offer different prices and services
- **Optional** — the free attention layer works without Lightning
- **Ecosystem-native** — sats flow between participants inside the network

## The Incentive Map

| Actor | Contributes | Earns (free layer) | Earns (Lightning layer) |
|-------|------------|-------------------|------------------------|
| User | Reliable storage for pact partners | Pact partners forward content → wider reach | Can pay relays for boost, priority, retention |
| Discovery relay | Curates quality content | Followers → influence → users publish through it | Earns sats from boosts, priority delivery |
| Infrastructure relay | Fast indexing, high availability | Users rely on it as accelerator → sticky base | Earns sats from extended retention, premium features |
| Community relay | Serves a group/topic | Community membership → exclusive content access | Earns sats from group-specific services |
| Passive user | Little/nothing | Basic WoT reach — network works, just smaller | Can pay to compensate for lack of organic reach |

## What Keeps Everyone Honest

- **Storage peers** — challenge-response + reliability scoring. Fail → lose pacts → lose forwarding advocates
- **Relays** — subscriber count. Bad curation or broken promises → users leave
- **Users** — reciprocity. Stop contributing → pact partners drop you → reach shrinks

Individual experience degrades gracefully. Network-level cooperation is fragile above ~30% defection — see fragility warning above. Simulation evidence indicates the fragility threshold may be even lower: net pact contraction occurs at 0% defection under realistic conditions, suggesting the scoring mechanisms themselves need tuning before the honesty feedback loop can sustain a cooperative equilibrium.

## Fragility and Limitations

The incentive model has structural limitations that should be acknowledged:

**The lurker gap.** Bilateral pacts require both parties to produce content. Read-only users (60-80% of typical social platforms) cannot participate in the pact economy. The incentive model covers the 10-20% who create content. The remaining users are permanent relay consumers. This is acceptable — lurkers have minimal data to protect — but it means the "data sovereignty for everyone" framing overstates the protocol's reach.

**The 30% tipping point.** Game-theoretic analysis shows the cooperative equilibrium collapses when free-riding exceeds approximately 30%. At 50% free-riding, data availability degrades measurably. At 70%, the network is functionally unreliable. The reach gradient (more pacts → more forwarding) is the primary defense, but its magnitude may be insufficient for users who don't value wider reach. Simulation evidence strengthens this concern: pact networks contract under purely cooperative behavior (zero defectors), implying the effective tipping point is below 30% — possibly at 0% — depending on topology and availability patterns. The volume-tolerance and reliability scoring thresholds may need relaxation (e.g., delta > 0.30, longer scoring windows) to prevent the protocol's own mechanisms from triggering contraction.

**Volume matching creates activity-band segregation.** The +/-30% volume tolerance means power users (100+ events/day) can only pair with similar-volume users. Users at extremes of the activity distribution may struggle to find compatible partners within their 2-hop WoT. Simulation evidence suggests this mechanism is a significant driver of pact dissolution even without strategic defection — natural activity variance causes nodes to drift outside each other's tolerance bands, triggering pact drops that compound across the network.

**Guardian pacts have no Nash equilibrium for volunteering.** Guardian pacts are one-sided: the Guardian stores a Seedling's data and receives nothing in return. The unique Nash equilibrium is "nobody volunteers." The pay-it-forward framing relies on prosocial behavior, not rational self-interest. Operational deployment requires either Genesis Guardian infrastructure or a guardian incentive mechanism.

**Relay economics during transition.** As pacts mature and relay traffic decreases, relay operators lose their primary value (storage revenue) before the Lightning premium layer generates replacement income. This creates a "valley of death" where relay sustainability is threatened during the transition from relay-dependent to sovereign operation. Relays must remain economically viable because they are permanently needed for content discovery and cross-community content.

**Topology-dependent stability.** Simulations reveal that network topology has a major impact on pact stability. Sparse hub-and-spoke graphs (BA m=10) exhibit churn of 2.79 pact changes/node/day — roughly 2.5x lower than dense graphs (BA m=50 at 6.87/node/day). Adding timezone-correlated availability patterns further increases churn to 8.04/node/day in dense topologies. This implies the incentive model's stability depends heavily on the emergent topology of the WoT graph. Networks that naturally form around well-connected hubs (influencers, community leaders) may sustain the cooperative equilibrium more easily than uniformly dense or timezone-fragmented networks. Protocol design should consider whether to actively encourage hub formation (e.g., higher pact limits for reliable high-degree nodes) or whether this creates unacceptable centralization.

## Rejected Alternatives

**Blind contribution tokens** — pact partners issue blind tokens for passing storage challenges. Accumulate tokens for priority. Rejected: significant protocol complexity (blind signatures, anonymous credentials), gameable via colluding pact partners.

**Public reputation tiers** — coarse-grained public contribution level attested by threshold signature. Rejected: reveals storage participation metadata, requires threshold signature scheme, still gameable.
