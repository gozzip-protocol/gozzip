# Genesis Bootstrap Plan

**Date:** 2026-03-14
**Status:** Draft
**Addresses:** Agent 04 (Cold Start) Issues 1-2, Agent 03 (Game Theory) Issue 3, Synthesis Verdict §1.1

## Problem Statement

At network launch:
- Zero users are in Sovereign phase (15+ pacts)
- Therefore zero users qualify as Guardians
- Therefore the guardian pact mechanism cannot function
- Bootstrap pacts (triggered by follows) are the sole onboarding mechanism
- Bootstrap pacts require the followed user to accept — but early adopters have no pact infrastructure

The protocol needs temporary centralized scaffolding to bridge the gap between "zero users" and "self-sustaining pact network."

## Genesis Guardian Nodes

### Design

Deploy 5-10 operator-controlled full nodes that serve as:
1. **Guardian pact partners** for all Seedlings during the first 6-12 months
2. **Bootstrap pact partners** for early adopters who follow the Genesis accounts
3. **High-uptime pact partners** for users in the Growing phase

### Specifications

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Node count | 5-10 | Enough for redundancy; few enough for transparency |
| Operator | Project team or trusted community members | Must be publicly identified |
| Uptime target | 99.5% | Higher than the 95% Keeper baseline |
| Guardian capacity | Up to 100 Seedlings per node | ~70 MB storage at active-user volumes |
| Bootstrap capacity | Up to 500 bootstrap pacts per node | ~340 MB storage |
| Geographic distribution | At least 3 continents, 5 timezones | Uptime complementarity for global users |
| Hardware | VPS, 4 GB RAM, 100 GB SSD, 1 Gbps | Modest requirements |
| Monthly cost estimate | $20-50 per node | Standard VPS pricing |

### Identity and Transparency

Genesis Guardian nodes are publicly identified:
- Each node has a well-known pubkey listed in project documentation
- Each node's kind 10002 relay list, kind 10050 device delegation, and pact statistics are publicly auditable
- A transparency dashboard publishes monthly reports:
  - Number of Seedlings served
  - Number that reached Hybrid/Sovereign phase
  - Average time to Hybrid phase
  - Uptime metrics
  - Storage consumed

### Guardian Behavior

Genesis nodes behave identically to organic Guardians with relaxed constraints:
- Accept guardian pacts from any Seedling (no WoT requirement — they are Genesis)
- Store Seedling events for up to 90 days or until Hybrid phase
- Respond to challenge-response verification like any pact partner
- Forward Seedling content via gossip with active-pact-partner priority
- Do NOT censor, filter, or prioritize specific Seedlings

### Bootstrap Pact Behavior

Genesis nodes also serve as bootstrap pact partners:
- Any user who follows a Genesis node pubkey triggers a bootstrap pact
- This provides immediate storage redundancy for early adopters
- Genesis nodes accept bootstrap pacts up to capacity (500 per node)

## Network Health Metrics

### Metrics to Track

| Metric | Definition | Healthy Threshold |
|--------|-----------|-------------------|
| Sovereignty ratio | % of users in Sovereign phase (15+ pacts) | >10% at 6 months |
| Guardian supply/demand | Organic Guardians / Seedlings needing guardians | >1.0 for sunset |
| Pact formation rate | New pacts formed per day / users needing pacts | Positive trend |
| Full-node ratio | % of users running full nodes | >5% |
| Relay dependency | % of reads served by relays vs. pact partners | Decreasing trend |
| Churn rate | % of users leaving per month | <15% |

### Sunset Conditions

Genesis Guardian nodes are removed when ALL of the following are met:
1. **Organic guardian supply exceeds Seedling demand** (supply/demand ratio > 1.5 for 30 consecutive days)
2. **Network has at least 1,000 Sovereign-phase users**
3. **Sovereignty ratio exceeds 15%** (enough users have mature pact networks)
4. **Pact formation rate is self-sustaining** (new users reach Hybrid phase within 60 days without Genesis assistance)

Sunset is gradual:
1. Genesis nodes stop accepting NEW guardian pacts (existing ones continue)
2. After 90 days, all guardian pacts expire naturally
3. Genesis nodes transition to regular Keeper nodes (still participate in the network as peers)
4. Genesis pubkeys are retained for historical transparency

### Failure Conditions

If after 12 months:
- Sovereignty ratio is below 5%
- Organic guardian supply is near zero
- Genesis nodes serve >80% of all guardian pacts

Then the bootstrap strategy has failed. Possible responses:
- Extend Genesis operation with increased capacity
- Re-evaluate the guardian incentive model (see guardian-incentives.md)
- Consider relay-as-Guardian integration (relays volunteer guardian slots as a service)
- Acknowledge that the current incentive model may need fundamental revision

## Early Network Parameter Relaxation

During the bootstrap period (network < 1,000 users), certain protocol parameters should be relaxed:

| Parameter | Normal Value | Bootstrap Value | Rationale |
|-----------|-------------|-----------------|-----------|
| Volume tolerance | ±30% | ±100% | Thin matching pool; accept wider range |
| Follow-age for pacts | 30 days mutual | 7 days mutual | Accelerate pact formation |
| Guardian threshold | Sovereign (15+ pacts) | 5+ pacts | Lower barrier to becoming a Guardian |
| Min account age | 7 days | 3 days | Faster onboarding for early adopters |

Parameters tighten automatically as network size grows:
- At 500 users: follow-age increases to 14 days
- At 1,000 users: volume tolerance tightens to ±50%, follow-age to 21 days
- At 3,000 users: all parameters at normal values

## Adoption Strategy Integration

Genesis Guardian nodes are necessary but not sufficient. The bootstrap plan must be paired with:

1. **Day-one value features** that work without pacts: multi-device identity, encrypted DMs, social recovery (see whitepaper §10.1)
2. **Community-first targeting**: dense clusters of 200+ users (conference attendees, activist groups, Bitcoin communities) rather than scattered individual adoption
3. **The first client must be the best Nostr client available**: users adopt for UX, sovereignty accrues in the background

## Cost Estimate

| Item | Monthly Cost | Duration | Total |
|------|-------------|----------|-------|
| 10 VPS nodes | $200-500 | 12 months | $2,400-6,000 |
| Monitoring/dashboard | $50 | 12 months | $600 |
| Operator time | volunteer or project-funded | 12 months | — |
| **Total** | | | **$3,000-7,000** |

This is a modest investment for bootstrapping a decentralized network. Comparable projects (Mastodon, Matrix) required similar or larger investments in initial infrastructure.
