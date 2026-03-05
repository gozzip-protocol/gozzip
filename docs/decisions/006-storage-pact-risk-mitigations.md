# ADR 006: Storage Pact Risk Mitigations

**Date:** 2026-02-28
**Status:** Accepted

## Context

The storage pact layer (ADR 005) introduces reciprocal storage commitments between WoT peers. Risk analysis identified 13 failure modes across security, availability, privacy, and incentive alignment. These need concrete mitigations before the design is production-ready.

Three risks are critical (P0):
- **Completeness** — signatures prove authenticity but not that all events are present
- **Cold start** — new users have no WoT peers to form pacts with
- **Eclipse attack** — attacker becomes majority of a user's storage peers

### Bootstrap lifecycle

Cold start is addressed through two complementary mechanisms — bootstrap pacts and guardian pacts — that together carry a *Seedling* from zero peers to self-sustaining:

1. Seedling joins (0 pacts)
2. Guardian pact forms (1 peer) — a *Guardian* volunteers storage
3. First follow → bootstrap pact (2 peers) — followed user stores Seedling's data
4. WoT builds → reciprocal pacts form (3+ peers)
5. At 5+ pacts (Hybrid phase): guardian pact expires
6. At 10+ pacts: bootstrap pact expires
7. Self-sustaining — all storage through reciprocal WoT pacts

## Decision

Address all 13 risks. Only 4 require protocol changes; the remaining 9 are client-side logic.

### Protocol changes

1. **Merkle root in checkpoint** — add `merkle_root` tag to kind 10051, enabling completeness verification
2. **Pact type variants** — add `type` tag to kind 10053 (`bootstrap`, `archival`, `guardian`) and `status` tag (`standby`)
3. **Serve challenge mode** — add `type` tag to kind 10054 (`hash`, `serve`) for latency-based fraud detection
4. **Blinded data requests** — replace `p` tag in kind 10057 with `bp` (blinded pubkey) using daily-rotating hash

### Client-side mitigations (no protocol changes)

5. WoT cluster diversity for peer selection (eclipse prevention)
6. Peer reputation scoring (identity age, challenge success rate)
7. Graduated reliability scoring (replace binary pass/fail)
8. Popularity-scaled pact counts (serving asymmetry)
9. Per-peer request rate limiting (serving asymmetry)
10. Warm standby pact promotion (rebalancing window)
11. Pact offer WoT/age/volume filtering (spam prevention)
12. Independent checkpoint Merkle verification (checkpoint trust)
13. Jittered response coordination (response flooding)

## Consequences

- Completeness is now provable — requesters can verify they received all events
- New users can bootstrap via first-follow temporary pacts
- Eclipse attacks require penetrating multiple WoT clusters
- Data request privacy preserved via blinded identifiers
- 9 of 13 mitigations are client-side only — backward compatible, incrementally adoptable
- 4 schema additions are additive — old clients ignore new tags
- Guardian pacts provide a second cold-start mitigation path alongside bootstrap pacts — newcomers can receive storage from a volunteer *Guardian* before forming any WoT edges
