# ADR 015: Renewal-by-Default Pacts and a Four-State Machine

**Date:** 2026-07-29
**Status:** Accepted

## Context

Two problems compounded each other:

1. **The pact set was contracting.** The [game-theory review](../design/reviews/review-game-theory.md) (Issue 1) and [protocol-design review](../design/reviews/review-protocol-design.md) (Issue 8) found net-negative pact churn in every simulated topology: pacts dissolved quickly (on volume drift, on transient failures) but formed slowly, so the cooperative equilibrium was a "slow-motion collapse."
2. **The state machine was too large.** The [complexity review](../design/reviews/review-complexity.md) flagged the seven-state pact FSM (with standby promotion, renegotiation, partition handling) as more machinery than the mechanism warrants.

## Decision

**Renewal by default.** A healthy pact at its 90-day mark **auto-renews** via a lightweight handshake. Dissolution requires *sustained* failure ([ADR 014](014-presence-aware-reliability.md): Failed ≥14 days) or an explicit exit by either party. Pacts persist like real relationships — maintained unless actively broken — instead of requiring continuous requalification.

**Four-state FSM.** Collapse the pact lifecycle to:

```
Forming → Active → Failing → Ended
```

- **Forming** — offer sent/accepted, initial checkpoint exchange.
- **Active** — challenges passing (Healthy/Degraded); auto-renews at 90 days.
- **Failing** — reliability Unreliable/Failed; replacement being sought.
- **Ended** — dropped (sustained Failed) or explicit exit.

**Standby and archival pacts are deferred** (removed from the core). Instant failover is no longer needed once renewal-by-default keeps the ~20-pact set stable. The **Guardian pact** lifecycle is one paragraph: it expires at 90 days or when the Seedling reaches Hybrid phase (5+ pacts) — the newcomer has grown roots.

## Consequences

**Positive:**
- Turns the slow-motion collapse into a ratchet toward stability — the single most important game-theory fix.
- Four states instead of seven; no standby-promotion logic, no separate renegotiation path.
- Matches the human story: friendships are renewed by default, not re-earned every quarter.

**Negative:**
- Loses instant standby failover; a failed partner is replaced over the challenge window rather than swapped atomically. Acceptable given the redundancy target.
- Auto-renewal must be cheap and abuse-resistant (lightweight signed handshake, not a full renegotiation).

**Neutral:**
- The formation state machine from the old equilibrium model is dissolved into the three-phase adoption arc ([ADR 016](016-target-based-pact-formation.md)), leaving exactly two lifecycles: the user arc (Bootstrap→Hybrid→Sovereign) and this pact FSM.
