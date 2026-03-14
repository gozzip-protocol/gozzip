# Adversarial Review: Game Theory and Mechanism Design

**Reviewer:** Game Theory and Mechanism Design Specialist
**Date:** 2026-03-14
**Scope:** Nash equilibria, cooperative stability, mechanism design adequacy, incentive compatibility, strategic manipulation surfaces
**Documents reviewed:** Whitepaper v1.2 (gossip-storage-retrieval.tex), incentive-model.md, equilibrium-pact-formation.md, guardian-incentives.md, genesis-bootstrap.md, plausibility-analysis.md

---

## Executive Summary

The Gozzip protocol attempts to build decentralized storage infrastructure on a foundation of bilateral reciprocity, using reach (gossip forwarding priority) as the primary incentive currency. This review finds that **the cooperative equilibrium the protocol depends on is not merely fragile -- it is likely not an equilibrium at all** under the protocol's own rules. Simulation data confirms this: net-negative pact churn in ALL tested topologies means the system is contracting, not converging to a steady state. The protocol has seven distinct mechanism design problems, of which three are severity-critical (the system cannot work without fixing them) and four are severity-significant (the system works poorly without fixing them). The core difficulty is that the protocol tries to extract infrastructure-level reliability from social-level incentives, and the gap between these two domains manifests as churn, defection surfaces, and equilibrium collapse.

The protocol's intellectual honesty is exceptional -- the design documents preemptively identify most weaknesses this review raises. But diagnosis is not treatment. The recommendations below focus on mechanism design fixes, not exhortations to "monitor and adjust."

---

## Issue 1: The Cooperative Equilibrium Is Not Stable -- It Is a Slow-Motion Collapse

**Severity: Critical**

### The Problem

The protocol's core claim is that rational nodes will cooperate because cooperation yields reach (more pact partners = more forwarding advocates). The game-theoretic model posits two stable equilibria: universal cooperation and universal defection. The protocol's goal is to keep the system in the cooperative equilibrium via a reach gradient that makes defection costly.

Simulation falsifies this model. In **every topology tested** (BA m=10, WS p=0.30, BA m=50, BA m=50+TZ), pact churn is net-negative over 30 days. The numbers are stark:

| Topology | Net pact change (30 days) | Churn/node/day | Availability |
|----------|--------------------------|----------------|-------------|
| BA m=10 | -40,030 | 2.79 | 98.4% |
| WS p=0.30 | -45,710 | 5.45 | 97.7% |
| BA m=50 | -104,444 | 6.87 | 95.3% |
| BA m=50+TZ | -112,869 | 8.04 | 94.8% |

These simulations model **zero strategic defectors** -- all nodes behave cooperatively. Yet the network contracts. This means the cooperative equilibrium is not a fixed point; it is a transient state that decays toward lower pact counts under the protocol's own mechanisms. The "30% defection tipping point" identified in the analytical model is irrelevant -- the system is tipping at 0% defection.

### Root Cause Analysis

The contraction has three compounding causes, all rooted in mechanism design:

1. **Volume matching is a pact destruction machine.** With delta=0.30, natural activity variance (a user posts 25 events one week and 40 the next) pushes partners outside tolerance bands. The pact dissolves not because of defection but because of ordinary human behavior. Volume matching was designed to prevent exploitation, but it treats natural fluctuation as a pact violation.

2. **Reliability scoring penalizes heterogeneity.** The alpha=0.95 EMA with a hard 50% drop threshold means a Witness partner who misses 4 consecutive challenges (statistically normal at 30% uptime -- the probability is (0.7)^4 = 24%) triggers a drop. The scoring system interprets normal Witness behavior as failure. The protocol's design documents acknowledge this ("light nodes at 30% challenge success: below the 50% failed threshold -- dropped immediately") but treat it as a client-side optimization problem rather than a mechanism design flaw.

3. **The replacement pipeline is slower than the dissolution pipeline.** Dropping a pact is unilateral and immediate. Forming a pact requires mutual WoT membership, volume matching, 30-day follow-age, and both parties being in the right formation state. Dissolution is fast; formation is gated by social prerequisites. This asymmetry means the pact economy has a structural bias toward contraction.

### What This Means

The protocol does not have a cooperative equilibrium in the game-theoretic sense. It has a cooperative *initial condition* that decays. The system will converge to a lower pact count where dissolution rate equals the (slow) formation rate. At that lower equilibrium, relay dependency is higher, availability is lower, and the reach incentive is weaker -- creating a secondary contraction pressure.

### Recommendation

The contraction is not fixable by parameter tuning alone. The protocol needs a structural change: **asymmetric friction on pact dissolution vs. formation.**

Concrete options:
- **Grace periods with graduated penalties.** Instead of dropping a pact when reliability falls below 50%, freeze challenge scoring for 7 days and alert the partner. Only drop after sustained failure. This eliminates the Witness uptime false-positive problem.
- **Volume matching on trailing 90-day average, not current window.** Smooth out activity variance so weekly fluctuations do not trigger dissolution.
- **Automatic pact renewal with opt-out rather than opt-in.** Pacts should persist unless actively dissolved, not require continuous requalification.
- **Net pact count floor.** If a node's pact count drops below PACT_FLOOR, disable dissolution of existing pacts until the count is restored. This creates a one-way ratchet that resists contraction.

---

## Issue 2: Volume Matching (delta=0.30) Is Both Too Tight and the Wrong Mechanism

**Severity: Critical**

### Too Tight

The simulation churn data directly implicates volume matching as a primary driver of pact dissolution. At delta=0.30, a user whose partner's activity changes by more than 30% faces an automatic pact violation. Consider concrete scenarios:

- A user averages 20 events/day but has a busy week posting 30/day. Their volume rises from 450 KB/month to 675 KB/month -- a 50% increase. All pacts with partners in the 315-585 KB range are now out of tolerance. If 8 of 20 partners are in this range, the user faces 8 simultaneous pact violations from one active week.
- Seasonal variation (holidays, vacations, news events) creates periodic volume swings that systematically destabilize pacts. Every major event (election, crisis, conference) triggers a wave of pact renegotiation across the network.

The plausibility analysis acknowledges this: "natural activity variance causes nodes to drift outside each other's tolerance bands, triggering pact drops that compound across the network." But no fix is proposed.

### The Wrong Mechanism

Volume matching solves the wrong problem. The threat it addresses is asymmetric exploitation: a high-volume user "dumping" storage costs on a low-volume partner. But the actual cost of storing a partner's events is trivial -- the plausibility analysis shows 15.2 MB for an active user's entire pact set. On modern devices with 128-256 GB storage, the difference between storing 112 KB (casual partner) and 2.2 MB (power partner) is invisible to the user.

Volume matching creates real costs (pact churn, thin matching pools, activity-band segregation) to prevent a theoretical cost (asymmetric storage) that is economically negligible. This is a mechanism design error: the cure is worse than the disease.

### What the Simulation Data Suggests

The BA m=10 topology has the lowest churn (2.79/node/day) and the highest availability (98.4%). The BA m=50 topology has the highest churn (6.87/node/day) and the lowest availability (95.3%). Dense graphs give nodes more potential partners, which *increases* the rate of replacement cycling. This is the opposite of what the protocol predicts -- more options should mean better matches. The explanation: in dense graphs, small volume fluctuations expose more marginally-compatible partnerships to tolerance violations, creating cascading renegotiation.

### Recommendation

**Replace volume matching with asymmetric pact obligations.** Each partner stores whatever the other produces, with a hard cap (e.g., 10 MB/month per partner) to prevent abuse. The cap prevents exploitation; the absence of matching prevents churn from natural activity variance.

If volume matching is retained for philosophical reasons (the "reciprocal friendship" metaphor), widen delta to at least 0.60 and compute it over a 90-day trailing average rather than the current window. This absorbs seasonal variance and weekly fluctuations.

---

## Issue 3: Reliability Scoring (alpha=0.95 EMA) Does Not Detect Defection Fast Enough and Punishes Honest Witnesses

**Severity: Critical**

### Detection Lag for Strategic Defection

The EMA with alpha=0.95 means each new observation shifts the score by only 5%. Starting from a healthy score of 0.95, a defector who starts failing every challenge reaches the 50% drop threshold after:

```
0.95 * 0.95^n = 0.50
n * ln(0.95) = ln(0.50/0.95) = ln(0.526)
n = ln(0.526) / ln(0.95) = -0.642 / -0.0513 = 12.5
```

At one challenge per day, a complete defector is not dropped for **13 days**. A strategic partial defector who fails every other challenge (maintaining ~10 pacts worth of commitment) would see their score stabilize at:

```
score_stable = (1 - alpha) / (1 - alpha * success_rate)
For 50% success: score = 0.05 / (1 - 0.95 * 0.50) = 0.05 / 0.525 = 0.095
```

Wait -- that converges to 0.095, which is below the 50% drop threshold. Let me recalculate for a smarter strategy: fail 30% of challenges (responding to 70%).

```
For 70% success: score converges to 0.05 * 0.70 / (1 - 0.95 * 0.70 + 0.05 * 0.70)
```

Using the EMA formula, if a node passes 70% of challenges, its long-run score is approximately 0.70. This sits in the "unreliable" band (50-70%) which triggers "begin replacement negotiation" -- but replacement takes time (WoT discovery, volume matching, mutual acceptance). During the weeks or months of replacement negotiation, the partial defector continues to enjoy the reach benefits of 20 pact relationships while only serving 70% of obligations.

### False Positives on Honest Witnesses

The more severe problem is the false positive rate on Witnesses. A Witness with 30% uptime will succeed at approximately 30% of challenges (assuming presence-aware challenges are not yet implemented). At 30% success, the EMA converges to approximately 0.30 -- below the 50% "failed, drop immediately" threshold. This means **every Witness is algorithmically indistinguishable from a defector** under the current scoring system.

The design documents acknowledge this paradox but defer the solution to "client-side optimization" (presence-aware challenges). This is not a client optimization -- it is a fundamental mechanism design flaw. The reliability scoring system cannot distinguish "offline because mobile" from "offline because defecting" because it treats all challenge failures identically.

### The Strategic Partial Defection Game

The most damaging strategy is maintaining exactly 10-12 of 20 pacts. The defector:
1. Keeps their highest-value pact partners (Keepers, popular users, hub nodes)
2. Drops low-value partners (Witnesses, low-follower-count users, peripheral nodes)
3. Maintains sufficient pact count to stay above PACT_FLOOR (12)
4. Reduces storage and bandwidth obligations by 40-50%
5. Retains most forwarding benefits because retained pacts are with high-value nodes

This strategy is individually rational and collectively destructive. It is also undetectable: the defector's observable behavior (12 active pacts, all healthy) is identical to a legitimately smaller node. The protocol has no mechanism to distinguish "chose to have 12 pacts" from "strategically pruned to 12 pacts."

### Recommendation

Three complementary fixes:

1. **Presence-aware challenges are mandatory, not optional.** Challenges must only be sent when the peer has been recently observed online (via heartbeat, successful data exchange, or relay presence). Failed challenges to offline peers must not count against reliability scores. This is not a client optimization -- it must be specified in the protocol.

2. **Replace EMA with a window-based scoring model.** Track challenges over a 14-day sliding window. Score = (challenges passed when online) / (challenges sent when online). This eliminates the bias against low-uptime nodes and focuses on the actual question: "when this node is online, does it serve my data?"

3. **Detect partial defection through pact set analysis.** Track each node's pact count trajectory. A node that was at 20 pacts and is now at 12 without having reported partner failures should be flagged. Potential partners should be able to query a node's recent pact dissolution history before forming new pacts.

---

## Issue 4: Guardian Incentive Is Not Incentive-Compatible

**Severity: Significant**

### The Nash Equilibrium Is "Nobody Volunteers"

The design documents correctly identify this. The proposed fix (Mechanism 1: Persistent WoT Edge) provides a WoT edge after successful guardianship (Seedling reaches Hybrid phase). This is a delayed, probabilistic reward for an immediate, certain cost.

### Would I Volunteer as a Guardian?

No. The expected value calculation:

- **Cost:** Store ~100-700 KB/month for up to 90 days. Respond to challenges. The storage cost is trivial; the attention cost (monitoring, responding) is not.
- **Benefit:** One WoT edge to a new user (who, by definition, has a sparse WoT graph and therefore provides minimal routing value).
- **P(benefit realized):** P(Seedling reaches Hybrid phase within 90 days). For early-stage social platforms, user retention at 90 days is typically 20-40%. Call it 30%.
- **E[value] = 0.30 * (one low-value WoT edge) - attention_cost < 0**

The persistent WoT edge is worth more to the Seedling (who needs connections) than to the Guardian (who already has a dense graph). This is backwards -- the incentive should be valuable to the party bearing the cost.

### The Deeper Problem

Guardian pacts are a public good. The protocol frames them as bilateral (Guardian stores Seedling's data) but the beneficiary is the *network*, not the Guardian. The Seedling contributes nothing to the Guardian. The network benefits from onboarding new users. But the Guardian captures none of that network benefit.

Public goods problems have three known solutions: (1) coercion (mandate participation), (2) selective incentives (provide excludable rewards), (3) small group size (where reputation matters). The protocol attempts (2) with the WoT edge but the selective incentive is too weak. It implicitly relies on (3) -- the pay-it-forward social norm -- but this works only in small, tight-knit communities with repeated interaction. At scale, it fails.

### Recommendation

Convert guardian pacts from a public good to a club good:

- **Option A: Guardian relay integration.** Relays that operate Genesis Guardian services earn a "Guardian Relay" attestation that clients display prominently. Users who value the network's growth subscribe to Guardian Relays, providing economic support. The guardian function is funded by relay subscription revenue, not individual altruism.

- **Option B: Guardian credit system.** Every Sovereign-phase user accumulates "guardian credits" passively (one per month). Spending a credit (accepting a Seedling) unlocks a concrete, immediate benefit: priority challenge scheduling (your challenges are processed first by pact partners for 30 days), or a temporary doubling of the forwarding bonus. This converts the deferred-probabilistic reward to an immediate-certain one.

- **Option C: Accept that guardian pacts require permanent infrastructure.** The Genesis nodes are not temporary scaffolding; they are permanent community infrastructure like DNS root servers. Budget accordingly and remove the sunset conditions.

---

## Issue 5: PACT_FLOOR Creates a Misaligned Obligation and a Disguise Incentive

**Severity: Significant**

### The Misalignment

PACT_FLOOR=12 forces Keepers to maintain 12 active pacts when their self-interested equilibrium is ~7. The 5 excess pacts are a tax on Keepers for the benefit of Witnesses. The "forwarding bonus" (Design Decision 3 in equilibrium-pact-formation.md) is supposed to compensate, but:

1. The bonus is not quantified anywhere in the protocol specification. Its magnitude is undefined.
2. Even if defined, forwarding bonuses have diminishing returns. A user who already has 7 high-quality pact partners forwarding their content gains marginal benefit from 5 more low-quality Witness partners forwarding it.
3. The bonus is observable only through indirect effects (slightly wider reach). Unlike a direct payment, it is not attributable -- a Keeper cannot verify that their excess pacts caused measurable reach improvement.

### The Disguise Game

The rational strategy for a high-uptime node that wants to avoid PACT_FLOOR obligations is to declare itself a Witness (self-report <90% uptime in kind 10050) while maintaining actual 95% uptime. This "false Witness" strategy:

- Avoids the PACT_FLOOR=12 obligation (Witnesses reach comfort at ~35 pacts from other Witnesses, but with 95% actual uptime, they reach comfort at ~7 -- same as an honest Keeper)
- Earns high reliability scores from challenge-response (because actual uptime is 95%)
- Gets preferred as a pact partner (because high reliability) without bearing the PACT_FLOOR tax
- Is undetectable by the protocol: challenge-response measures availability, not self-declared node type

The prior review (agent-03-04) identified this and recommended basing PACT_FLOOR on observed uptime rather than self-declared type. I concur and add: the false Witness strategy is not just theoretically possible; it is the **unique Nash equilibrium** for any Keeper that has more than 7 compatible partners available. Declaring as a Keeper is strictly dominated by declaring as a Witness.

### The Sustainability Question

Even without the disguise game, PACT_FLOOR is unsustainable if the forwarding bonus is insufficient. The protocol asks Keepers to bear 71% more pacts than they need (12 vs 7) for an unquantified, indirect benefit. In mechanism design terms, this is a participation constraint violation: the mechanism demands more from Keepers than it returns to them.

If Keepers are the protocol's most valuable participants (they provide 95% uptime, they absorb disproportionate storage load, they are the backbone of availability), the protocol should be designed to *attract* Keepers, not tax them.

### Recommendation

1. **Quantify and guarantee the forwarding bonus.** Specify that nodes above PACT_FLOOR receive a measurable, verifiable benefit -- e.g., their events are forwarded at 1.5x priority by all pact partners, visible in the gossip forwarding rules.

2. **Base PACT_FLOOR on observed uptime, not self-declaration.** Any node with >85% challenge-response success over 30 days is a de facto Keeper, regardless of kind 10050 declaration. This eliminates the disguise incentive entirely.

3. **Consider replacing PACT_FLOOR with a market mechanism.** Instead of mandating excess pacts, let Witnesses pay Keepers for the privilege of partnership. The payment could be in forwarding priority: a Witness paired with a Keeper forwards the Keeper's content at maximum priority. This converts the PACT_FLOOR tax into a voluntary exchange.

---

## Issue 6: What Is Overcomplicated

**Severity: Significant**

The protocol's incentive layer has accumulated mechanisms that interact in ways that are difficult to reason about and likely impossible to tune simultaneously. Several could be simplified or removed.

### 6.1 The Equilibrium-Seeking Formation Model Is Elegant but Overfit

The Poisson binomial comfort condition (P(X_h < K) <= epsilon for all h in {0..23}) is mathematically beautiful and practically unnecessary. It requires:
- Per-hour uptime histograms for each pact partner (rolling 7-day window)
- Exact convolution computation (O(n^2) dynamic programming)
- Marginal value computation for each potential new partner
- Coverage score, overlap coefficient, and functional balance metrics
- A 6-state formation state machine with hysteresis

All of this to determine... how many pacts to form. The simulation shows that the actual steady-state pact count settles below the comfort condition prediction anyway (equilibrium-pact-formation.md, Simulation Finding 2). The protocol could replace the entire formation model with:

```
target_pacts = max(PACT_FLOOR, ceil(K / min_partner_uptime))
```

For K=3 and a typical mix of 95% and 30% uptime partners, this gives target_pacts = max(12, ceil(3/0.30)) = max(12, 10) = 12. Same result, zero per-hour computation, no state machine.

The elaborate formation model creates an illusion of precision in a system where the binding constraint is not "how many pacts do I need" but "can I find compatible partners at all."

### 6.2 Age-Biased Challenge Distribution Adds Complexity Without Solving the Real Problem

The challenge system targets 50% of challenges at the oldest third of stored data, 30% at the middle, 20% at the newest. This is designed to detect selective deletion of old data. But:

- The actual threat is not selective deletion -- it is complete defection (stop storing entirely) or proxy forwarding (re-fetch data from a relay before responding to challenges).
- The 500ms latency check on serve challenges is described in the whitepaper as "a weak heuristic" against proxy forwarding. It is: any node on a fast connection can fetch from a relay in <200ms and respond within the window.
- Age-biased distribution increases challenge complexity without addressing the actual attack vectors.

A simpler approach: random sampling from the full stored range with occasional full-range hash verification (Merkle root comparison against the published checkpoint). This detects both selective deletion and incomplete storage with a single mechanism.

### 6.3 The Three-Phase Adoption Model (Bootstrap/Hybrid/Sovereign) Conflates Protocol State with Deployment Phase

The protocol defines adoption phases (Bootstrap: 0-5 pacts, Hybrid: 5-15, Sovereign: 15+) that govern behavior (relay dependency, gossip participation). But these phases duplicate the formation state machine (Bootstrap/Growing/Comfortable/etc.) with different thresholds and different transitions. The user is simultaneously in two state machines (adoption phase and formation state) that are partially correlated but not synchronized.

This is unnecessary complexity. The formation state machine alone is sufficient: a node in the "Growing" state is functionally in the Hybrid adoption phase; a node in the "Comfortable" state is functionally Sovereign. Merge the two state machines.

### 6.4 The Forwarding Priority System Has Four Tiers That Could Be Two

The gossip forwarding priority system has four levels: active pact partners > 1-hop WoT > 2-hop WoT > unknown. The distinction between 1-hop and 2-hop WoT adds routing complexity but is difficult to observe empirically (the user cannot tell whether their content was forwarded at 1-hop or 2-hop priority). Simplify to two tiers: pact partners and WoT members. Everything else is not forwarded.

---

## Issue 7: The Incentive Currency (Reach) Is Unobservable and Unfalsifiable

**Severity: Significant**

### The Problem

The entire incentive model rests on the claim that "more pacts = more forwarding advocates = more reach." But reach is:

1. **Not measurable by the user.** There is no protocol mechanism for a user to determine how many nodes their content reached. The user cannot verify that cooperation yields more reach than defection.
2. **Not directly caused by pact count.** Reach depends on followers, content quality, timing, and WoT topology. A user with great content and 5 pacts may get more reach than a boring user with 20 pacts.
3. **Not excludable.** A defector who maintains 5 pacts still gets forwarded by those 5 partners. They lose 15 forwarding advocates but may retain 80%+ of their practical reach if the 5 retained partners are well-connected hubs.

The whitepaper itself identifies this: "whether a cooperator with 20 pacts gets 5% or 50% more reach than a defector with 5 pacts determines whether the cooperative equilibrium is robust or fragile. This differential needs explicit measurement."

This measurement has not been performed. The protocol's primary incentive mechanism is built on an unmeasured, unverified hypothesis.

### Comparison to Working Systems

Incentive mechanisms in functioning decentralized systems use observable, verifiable currencies:
- **Bitcoin:** Block rewards are deterministic and verifiable. You can compute exactly how much mining a block is worth.
- **Filecoin:** Storage payments are on-chain and attributable. Providers know their revenue per GB.
- **BitTorrent (tit-for-tat):** Upload rate is directly reciprocated with download rate. The incentive is immediate and measurable.

Gozzip's "reach" is none of these. It is a diffuse, indirect, unobservable effect. This does not mean it is zero -- social incentives are real -- but it means the protocol cannot rely on reach as a primary mechanism for sustaining cooperation among rational actors.

### Recommendation

Either:
1. **Make reach observable.** Add a protocol mechanism for nodes to report reach metrics (e.g., a "forwarding receipt" event that pact partners issue when they forward your content, aggregatable by the client). This makes the incentive tangible and verifiable.
2. **Add a secondary, observable incentive.** Lightning micropayments for challenge-response success (tiny -- 1 sat per successful challenge) would provide an immediate, measurable reward for cooperation. At 20 challenges/day, this is 20 sats/day -- economically trivial but psychologically effective as a cooperation signal.
3. **Adopt a direct reciprocity mechanism.** Tit-for-tat: "I forward for you exactly as much as you forward for me" with measurable forwarding counters. This is cruder than the current system but has proven equilibrium properties (Axelrod's tournament results apply directly).

---

## Alternative Approach: How I Would Redesign the Incentive Layer

The current design attempts to create a cooperative equilibrium through indirect social incentives (reach) in a system where the costs are concrete (storage, bandwidth, computation) and the benefits are diffuse (maybe more people see my posts). This asymmetry between concrete costs and diffuse benefits is the root cause of every issue identified above.

### Design Principle: Direct Reciprocity with Tolerance

Replace the current complex incentive stack with a simpler mechanism based on **direct, observable, bilateral reciprocity**:

### 1. Tit-for-Tat Storage

Each pact tracks a bilateral ledger: bytes stored for partner vs. bytes partner stores for you. The ledger allows a 3:1 imbalance (one partner can store up to 3x more than the other) before either party can dissolve. This replaces volume matching entirely:
- No activity-band segregation
- A casual user (112 KB/month) can pair with a power user (2.2 MB/month) as long as the casual user is willing to store 2.2 MB (trivial on any modern device)
- The 3:1 cap prevents extreme exploitation without triggering churn from normal variance
- Dissolution requires the imbalance to exceed 3:1, which only happens with genuine activity mismatch, not weekly fluctuation

### 2. Forwarding-for-Forwarding

Replace the abstract "forwarding bonus" with direct reciprocal forwarding: "I forward your events to my WoT at the same priority you forward mine." Forwarding counters are exchanged between pact partners periodically (kind event with forwarding stats). A partner who forwards less than 50% of what you forward for them gets deprioritized. This makes the reach incentive:
- Observable (you can count how often your partner forwarded your content)
- Proportional (more forwarding in = more forwarding out)
- Self-enforcing (no central authority needed)

### 3. Challenge Scoring Based on Availability Windows

Replace the EMA reliability score with a window-based system:
- Each node advertises availability windows (e.g., "I am typically online 08:00-23:00 UTC")
- Challenges are sent only during advertised windows
- Score = (challenges passed during window) / (challenges sent during window)
- A Witness with 30% total uptime but 90% uptime during their advertised 8-hour window scores 90%, not 30%
- This eliminates the false-positive problem for honest Witnesses and focuses detection on actual defection

### 4. Guardian Pacts as Relay Services

Remove guardian pacts from the peer-to-peer layer entirely. Guardian services are provided by relays, funded by:
- Community donations (Lightning, on-chain)
- Relay subscription revenue
- Project treasury during genesis

This converts the public goods problem to a straightforward service provision model. Relays already have infrastructure, uptime, and economic models. Adding "store one Seedling's data for 90 days" is a trivial extension of relay operation. The pay-it-forward social norm is a nice aspiration; relay-operated guardian services are a deployable solution.

### 5. Simplify the State Machine

Replace the 6-state formation machine and the 3-phase adoption model with a single 3-state model:
- **Seeking**: Need more pacts. Accept offers, send requests. (Combines Bootstrap and Growing.)
- **Stable**: Have enough pacts for comfort. Accept offers only from nodes that need help. (Replaces Comfortable.)
- **Pruning**: Over ceiling. Dissolve lowest-value pacts with 14-day notice. (Replaces Over-Provisioned.)

No hysteresis is needed because the tit-for-tat storage model eliminates most churn sources. Pacts dissolve only when: (a) a partner is unreachable for 14+ consecutive days, (b) the bilateral ledger exceeds 3:1 imbalance for 30+ consecutive days, or (c) either party voluntarily exits. All three are rare events compared to the current dissolution triggers.

### 6. PACT_FLOOR Based on Observed Behavior

PACT_FLOOR applies to any node whose observed uptime exceeds 85% over 30 days, regardless of self-declaration. This eliminates the false-Witness strategy entirely.

### Expected Properties

This redesigned incentive layer has:
- **Observable incentives** (bilateral storage ledger, forwarding counters)
- **Proportional costs and benefits** (store more = stored more for; forward more = forwarded more for)
- **Tolerance for heterogeneity** (3:1 storage imbalance allows Keeper-Witness pacts without volume matching)
- **Fewer mechanisms** (no volume matching, no EMA scoring, no age-biased challenges, no multi-state adoption phases)
- **Known equilibrium properties** (tit-for-tat is a known Nash equilibrium in iterated games; the protocol instantiates a standard result rather than hoping for an emergent one)
- **Solve the guardian problem economically** (relay services, not altruism)

The trade-off: this design is less "elegant" than the current one and less faithful to the human-community metaphor. It treats the protocol as infrastructure (which it is) rather than a social analogy (which it aspires to be). But infrastructure that works is better than a metaphor that contracts.

---

## Summary Table

| # | Issue | Severity | Core Problem |
|---|-------|----------|-------------|
| 1 | Cooperative equilibrium is not stable; net-negative pact churn in all topologies at 0% defection | Critical | Dissolution pipeline faster than formation pipeline; volume matching and reliability scoring cause organic contraction |
| 2 | Volume matching (delta=0.30) destroys pacts from normal activity variance | Critical | Solves a negligible problem (asymmetric storage cost) at enormous cost (systemic churn) |
| 3 | Reliability scoring (alpha=0.95 EMA) cannot distinguish Witnesses from defectors; partial defection is undetectable | Critical | Scoring mechanism conflates low uptime with defection; strategic partial defection is individually rational and invisible |
| 4 | Guardian incentive is not incentive-compatible; Nash equilibrium is "nobody volunteers" | Significant | Cost is immediate and certain; benefit is deferred, probabilistic, and low-value |
| 5 | PACT_FLOOR creates a disguise incentive for Keepers; forwarding bonus is unquantified | Significant | Self-declaration of node type is gameable; the obligation exceeds the reward |
| 6 | Multiple mechanisms are overcomplicated relative to their contribution | Significant | Equilibrium-seeking formation model, age-biased challenges, dual state machines, 4-tier forwarding priority add complexity without proportional benefit |
| 7 | The incentive currency (reach) is unobservable, unverifiable, and unfalsifiable | Significant | No protocol mechanism exists to measure whether cooperation actually yields more reach; the entire incentive model rests on an untested hypothesis |

---

## Closing Assessment

The Gozzip protocol has done something rare: it has built a simulation that falsifies its own assumptions, and it has published the results honestly. The net-negative pact churn data is the single most important finding in the entire project, and the design documents treat it with appropriate seriousness.

But the response to this finding has been diagnostic, not prescriptive. The documents note that "pact stability may matter more than pact abundance" and suggest "graduated penalties, renewal incentives, challenge grace periods" -- but none of these are specified, quantified, or tested. The protocol's core mechanisms (volume matching, reliability scoring, forwarding-based incentives) remain unchanged despite simulation evidence that they produce contraction.

The path forward is not more analysis. It is mechanism redesign. The three critical issues (equilibrium instability, volume matching churn, reliability scoring false positives) are not parameter tuning problems -- they are structural problems that require structural solutions. The alternative approach outlined above is one possible direction; others exist. But the current incentive layer, as specified, will not produce a stable cooperative equilibrium. The simulation has already shown this.
