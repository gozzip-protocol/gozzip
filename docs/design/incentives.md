# Incentives

**Status:** Accepted

## Principle: reach is the currency

Value in Gozzip comes from within the ecosystem. Attention and reach are the primary currency — contributing more to the network makes your content more discoverable. There is **no karma, no score, no token, no mandatory payment** ([ADR 009](../decisions/009-incentive-model.md)). Lightning zaps sit as an optional premium layer on top, never as the base.

The mechanism is pact-aware gossip routing. When a node decides what content to propagate, it prioritizes: active pact partners first (you store their data, so making their content discoverable protects the value of your pact), then 1-hop follows, then 2-hop contacts if capacity allows, and unknown pubkeys never forwarded. A user with 20 reliable pact partners has 20 nodes that eagerly forward their content; a user with five flaky pacts has fewer advocates. No score is ever published — the network topology *is* the incentive. When a pact drops, the dropped peer simply loses a forwarding advocate and their reach naturally shrinks.

## The pay-it-forward engine

The incentive loop is a moral engine, not a scoreboard. A newcomer joins with nothing to offer, is vouched for by a Guardian, stores reliably for the partners they accumulate, and earns wider reach as their content is forwarded. As they grow they gather followers, more potential partners, more advocates — and eventually they have the standing to vouch for the next newcomer themselves. **Today's Seedling becomes tomorrow's Guardian.** The generosity that bootstrapped them is the generosity they extend, and the cycle is self-reinforcing: the network's health is paid forward from each cohort to the next.

## Core mechanics

Four mechanics map cleanly onto human behavior. Each is recorded in its own ADR; this is the summary.

**Capped asymmetric pacts** ([ADR 013](../decisions/013-capped-asymmetric-pacts.md)). Pacts are not volume-matched. Each partner stores whatever the other produces, up to roughly 10 MB/month per partner — the cap is the only limit, with no tolerance band and no drift-triggered renegotiation. This matters for the narrative as much as the mechanism: volume matching meters friendship and excludes the 60–80% lurker majority who produce little content. Real friendships tolerate asymmetry — I help you move, you feed my cat — and admitting lurkers as full pact participants makes the community story true rather than aspirational.

**Presence-aware reliability** ([ADR 014](../decisions/014-presence-aware-reliability.md)). A partner is only scored against challenges issued while they were observed online. Score = passes ÷ challenges-issued-while-partner-observed-online, over a rolling 14-day window, sorted into four bands: Healthy ≥90%, Degraded 70–90%, Unreliable 50–70%, Failed <50%. A partner is dropped only when Failed is sustained for at least 14 days. This rescues the Witness persona: without presence-awareness, an honest 30%-uptime Witness would be indistinguishable from a defector. Presence-awareness makes intermittent participation a valid way to belong.

**Renewal by default** ([ADR 015](../decisions/015-renewal-by-default-pacts.md)). A healthy pact auto-renews at 90 days through a lightweight handshake; dissolution requires sustained failure or an explicit exit. Pacts persist like real relationships — maintained unless broken — rather than dissolving unless continuously requalified. The result is a ratchet toward stability.

**Target-based formation** ([ADR 016](../decisions/016-target-based-pact-formation.md)). A client forms pacts until it holds about **20 active** pacts (floor 12), replaces on failure, and prefers partners in different timezones and a mix of Keepers and Witnesses — stated as a heuristic, not an optimization. 20 is the modeling constant used throughout the analysis. At least 15% of a user's pacts (about 3 of 20) should be Keepers.

## Keepers carry the community

One structural insight matters more than any single mechanism. Keepers (always-on, high-uptime) and Witnesses (intermittent) reach personal comfort at very different pact counts: a user backed by Keepers is comfortable with a handful of partners, while a user backed only by Witnesses needs many more. But Keepers are the scarce resource everyone wants. If a Keeper stopped accepting pacts the moment its *own* availability was satisfied, Witnesses could never get enough Keeper partners, and the network would split into comfortable Keepers and starved Witnesses.

The pact floor prevents this. Every node maintains at least 12 active pacts regardless of its own comfort, so **Keepers accept pacts beyond their own comfort threshold, and that excess capacity is exactly what carries the Witnesses.** The floor is a generosity constraint: the strong members of a community hold more than they need so the whole village stays covered. This is the "strong members carry the community" plot point, and it is why the floor exists in story terms, not just arithmetic.

## Relays and the premium layer

Relays are permanent infrastructure with a reduced role, not a failure condition ([ADR 018](../decisions/018-honest-security-and-relay-framing.md)). Discovery relays curate content by topic or community and earn followers like a person does; infrastructure relays offer fast, high-availability serving; community relays serve specific groups. A relay has its own pubkey, publishes curated lists, and reaches its subscribers' feeds through gossip. Publishing through a well-connected relay puts your content in front of an audience beyond your own WoT.

Lightning adds an optional premium tier on top of the free attention layer: relays publish a service menu (priority delivery, extended retention, transparently-marked content boosts) and users zap to activate them. Boosted content is always visibly marked, relays compete on price and features, and the free layer works without any of it. Sats flow between participants inside the network — no external subscriptions.

## Guardians and the genesis founding-elders

A **Guardian pact** is one-sided by design: a Sovereign-phase user (15+ pacts) volunteers to store one Seedling's data and receives nothing directly in return. It uses kind 10053 with `type: guardian`, each Guardian holds at most one active guardian pact, and it expires at 90 days or when the Seedling reaches Hybrid phase (5+ reciprocal pacts). This is community patronage: a settled member vouching for a newcomer who cannot yet reciprocate.

When the Seedling reaches Hybrid, the guardian pact simply ends — **the Seedling has grown roots.** They now have reciprocal pacts of their own and no longer need a patron. The graduation is a moment in the story even though it is not an event on the wire: the state machine expresses it through pact expiry. The rite is social, not a message.

**The genesis problem — the founding elders.** For the first 6–12 months there are no Sovereign-phase users, so organic Guardian supply is zero: no one has yet grown roots deep enough to shelter anyone else. Genesis Guardian nodes fill this gap as temporary patrons — the network's founding elders, run to bootstrap the first community into existence. During the genesis period the Guardian threshold is relaxed from 15+ pacts to 5+ (see [genesis-bootstrap.md](genesis-bootstrap.md)); as real Sovereign-phase users emerge, they take over the role and the elders step back. Genesis-era scaffolding is framed as scaffolding, never as a replacement for organic guardianship.

## Honest limitations

The incentive model has structural weaknesses, and naming them is part of keeping the story honest.

**The Guardian pact has no Nash equilibrium for volunteering.** Because the Guardian receives nothing in exchange, the unique Nash equilibrium of the one-shot game is "nobody volunteers" — a classic public-goods problem. The pay-it-forward framing relies on prosocial behavior, which demonstrably exists (open-source maintainers, Wikipedia editors, forum moderators) but is risky to build critical infrastructure on. The v1 answer is Genesis Guardian nodes plus the persistent-social-connection benefit of a completed guardianship, not an elaborate incentive scheme.

**The lurker gap.** Bilateral pacts require both parties to produce content, so the reach reward has little value for read-only users. Capped asymmetric pacts let lurkers participate on the storage side, but the "data sovereignty for everyone" framing still overstates the protocol's reach — a consumer mode (relay-dependent, few or no pacts) should be a first-class experience, not a degraded one.

**Cooperation is fragile at the network level.** Game-theoretic analysis puts the cooperative equilibrium's tipping point near 30% free-riding. The four mechanics above are chosen specifically to relax the self-inflicted pressures — volume metering and aggressive scoring thresholds — that would otherwise drive contraction even without strategic defection; whether cooperation holds at scale remains an open, honestly-flagged question.

**Relay economics during transition.** As pacts mature and relay storage traffic falls, relay operators can lose their primary value before the premium layer replaces it. Relays must stay economically viable because they are permanently needed for discovery and cross-community reach.
