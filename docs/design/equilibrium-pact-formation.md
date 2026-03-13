# Equilibrium-Seeking Pact Formation

How the protocol determines the right number of pacts for each user through a mathematically-derived equilibrium.

## Motivation

A fixed pact count cannot capture the availability reality of a heterogeneous network. A user with 20 pacts of Keepers (95% uptime) has vastly different availability than a user with 20 pacts of Witnesses (30% uptime). The protocol forms pacts until it reaches a measurable comfort threshold, then stops. The pact count is an emergent property of each user's specific partner composition.

## Comfort Condition

The protocol seeks an equilibrium where data is available with high confidence at every hour of the day. Formally:

**Comfort condition:** For every hour h in {0..23}:

```
P(X_h < K) ≤ ε
```

Where:
- `X_h` is the number of online pact partners during hour h
- `K = 3` (minimum online partners required)
- `ε = 0.001` (one-in-a-thousand failure probability per hour)

This means: at every hour of the day, the probability of having fewer than 3 online partners must be ≤ 0.001.

## Statistical Model

### Poisson Binomial Distribution

At any given hour h, each pact partner i is online with probability `p_i(h)`, derived from their uptime histogram (rolling 7-day challenge-response data). The count of online partners X_h follows a **Poisson binomial distribution** — a sum of independent but non-identically distributed Bernoulli variables.

Unlike the simple binomial (where all p_i are equal), the Poisson binomial accounts for the fact that a Keeper at 95% uptime and a Witness at 30% uptime contribute very differently to availability.

### Normal Approximation — When It Works and When It Doesn't

For large n, the Poisson binomial is well-approximated by N(μ, σ²) where μ = Σp_i and σ² = Σp_i(1-p_i). However, the **Berry-Esseen bound** shows this approximation is unreliable at the tails (which is exactly where ε = 0.001 lives) when n < 50.

For n ≤ 15: **Use exact convolution** (O(n²) dynamic programming, trivial for n ≤ 50).
For n > 15: Normal approximation with continuity correction is acceptable.

Clients should implement the exact method — it runs in microseconds for n ≤ 50.

### Equilibrium Counts by Composition

The number of pacts required to satisfy the comfort condition depends entirely on pact composition:

| Pact Composition | Per-partner uptime | Equilibrium pact count | Mean online at worst hour |
|---|---|---|---|
| All Keepers (95%) | 0.95 | ~7 | ~6.7 |
| 50% Keeper / 50% Witness | 0.625 avg | ~10 | ~6.2 |
| 25% Keeper / 75% Witness (typical) | 0.46 avg | 14–20 | ~9.2 |
| All Witnesses (30%) | 0.30 | 33–40 | ~10 |
| Same-timezone Witnesses | 0.05 (night) | Fails at 20 | Coverage gap |

**Key insight:** The required mean online count at the worst hour is approximately **10**, regardless of composition. This is a universal constant of the comfort condition at K=3, ε=0.001.

### Correlated Failures

Independent failures are an optimistic assumption. The **beta-binomial model** captures correlated failures (shared timezone, ISP, OS updates):

| Correlation ρ | Effect on required pacts |
|---|---|
| 0 (independent) | Baseline |
| 0.10 (mild) | ~2.6× more pacts needed |
| 0.20 (moderate) | ~10× more pacts needed |

This makes **uptime complementarity** and **geographic diversity** not just nice-to-haves but mathematically essential — they reduce ρ toward zero, keeping the required pact count tractable.

## The Asymmetric Equilibria Problem

The most important design insight: **Keepers and Witnesses reach comfort at vastly different pact counts.**

- A user with all Keeper partners is comfortable at ~7 pacts
- A user with all Witness partners needs ~35 pacts

But Keepers are the scarce resource everyone wants. If a Keeper reaches comfort at 7 pacts, they have no self-interested reason to accept more. This creates a structural asymmetry:

- Keepers stop accepting at ~7 → Witnesses can't get enough Keeper partners
- Witnesses need ~35 pacts of other Witnesses → enormous overhead
- The network splits into comfortable Keepers and uncomfortable Witnesses

### Solution: PACT_FLOOR = 12

Every node maintains at least 12 active pacts, regardless of comfort level. This ensures:

1. Keepers accept pacts beyond their own comfort threshold (because 12 > 7)
2. The extra pacts serve the network — a Keeper's excess capacity helps Witnesses reach comfort
3. The floor is not arbitrary: 12 is the smallest integer where a pact set of mixed composition (50/50 Keeper/Witness) reliably satisfies the comfort condition

The floor operates as a **generosity constraint**: comfortable nodes continue accepting pacts up to the floor, providing availability that the network needs.

## Formation State Machine

The protocol uses a 6-state formation controller with hysteresis to prevent thrashing:

```
BOOTSTRAP → GROWING → COMFORTABLE → OVER_PROVISIONED
                ↑          ↓
              DEGRADED ← ←
                ↓
             CEILING
```

### States

| State | Entry condition | Behavior |
|---|---|---|
| BOOTSTRAP | 0 pacts | Accept all valid offers, form aggressively |
| GROWING | 1+ pacts, comfort not met | Actively seek pacts, accept most offers |
| COMFORTABLE | ∀h: P(X_h < K) ≤ ε AND n ≥ FLOOR | Stop seeking. Accept offers only if partner needs help (their comfort not met) |
| OVER_PROVISIONED | n > CEILING (default 40) | Begin dissolving lowest-value pacts |
| DEGRADED | Was comfortable, comfort lost (partner dropped, uptime changed) | Seek replacement, prioritize filling coverage gaps |
| CEILING | n = CEILING, comfort not met | Cannot form more pacts. Log warning. Geographic diversity likely insufficient. |

### Hysteresis

To prevent oscillation between COMFORTABLE and DEGRADED:

- **Recruit threshold:** P(X_h < K) > ε = 0.001 (enter DEGRADED)
- **Dissolve threshold:** P(X_h < K) < ε/10 = 0.0001 (allow dissolution in OVER_PROVISIONED)

The 10× gap means a pact set doesn't start dissolving until it's significantly over-provisioned, and doesn't stop recruiting until it's significantly comfortable.

### Marginal Value of Next Pact

When deciding whether to seek another pact, the client computes the marginal improvement in comfort from adding one more partner with estimated uptime u:

```
Δ(n) = C(n, K-1) · u^K · (1-u)^(n-K+1)
```

Stop seeking when Δ(n) < ε/100. This prevents forming pacts that provide negligible availability improvement.

## Design Decisions

### 1. Bootstrap Overshoot

New users target 17 pacts initially (FLOOR + 5), not FLOOR. This provides a buffer for early pact failures and ensures the node has enough data to compute reliable uptime histograms before pruning down to equilibrium.

### 2. Reliability-Weighted Gossip

Pact partners with higher reliability scores (from challenge-response history) get higher gossip forwarding priority. This creates a measurable reach advantage for reliable nodes.

### 3. Forwarding Bonus

Nodes that generously accept pacts beyond their own comfort threshold receive a forwarding bonus — their content is propagated with slightly higher priority by their pact partners. This makes the PACT_FLOOR socially rewarding, not just obligatory.

### 4. Dissolution Notice Period

When dissolving a pact (OVER_PROVISIONED state), the node gives 14 days notice before stopping storage. This allows the partner to find a replacement without a coverage gap.

### 5. Pact Tenure Weighting

Older pacts (longer tenure) are weighted more heavily in the dissolution decision — newer, less-proven pacts are dissolved first. This rewards long-term reliability and prevents churn.

### 6. Flat Forwarding Penalty for Dissolution

A node that dissolves a pact without cause (partner was reliable, not over ceiling) loses forwarding priority from the dissolved partner. This makes premature dissolution costly.

### 7. Serving Load for Popular Accounts

For accounts with many followers, availability alone doesn't determine pact count — serving throughput matters:

```
K_eff = max(K_avail, ⌈R_peak / S⌉)
```

Where K_avail is the availability-driven K, R_peak is peak request rate, and S is per-peer serving capacity.

- Under 5K followers: availability dominates, K_eff = K_avail
- Above 5K followers: throughput dominates, more pacts needed for serving capacity
- Cascading read-caches handle the tail beyond K_eff

## Protocol Parameters

| Parameter | Value |
|---|---|
| PACT_FLOOR | 12 |
| PACT_CEILING | 40 |
| K (min online) | 3 |
| ε (failure prob) | 0.001 |
| Comfort check | ∀h: P(X_h < K) ≤ ε |
| Equilibrium count | Emerges from pact composition |

The protocol keeps forming pacts until comfortable, stops, and only dissolves when significantly over-provisioned. The actual pact count is an emergent property of the user's specific partner mix.

## Cold Start

New nodes have no uptime histogram data for potential partners. The protocol handles this with a **prior-seeded uptime estimate**:

- Keeper (declares 90%+ in kind 10050): prior = 0.85 (conservative discount)
- Witness (declares < 90%): prior = 0.25 (conservative discount)

As challenge-response data accumulates, the prior is replaced by empirical uptime. After 7 days of data, the prior weight drops below 10%.

## Anti-Thrashing

The formation state machine includes multiple anti-thrashing mechanisms:

1. **Hysteresis** (10× gap between recruit and dissolve thresholds)
2. **Jittered renegotiation** (0–48h random delay before replacement)
3. **Standby pool** (3 standby pacts absorb short-term failures)
4. **Dissolution notice** (14-day window)
5. **Minimum tenure** (don't dissolve pacts younger than 7 days)

Together, these ensure that the pact topology evolves slowly toward equilibrium rather than oscillating.
