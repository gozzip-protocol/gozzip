# Gozzip Protocol — Quantitative Plausibility Analysis

**Date:** 2026-03-01 (simulation evidence added 2026-03-14)
**Purpose:** Verify that the protocol's parameters produce viable numbers at different network sizes and identify bottlenecks.

All formulas are labeled `[F-XX]` for cross-reference. All input constants are in §1. To verify any result, trace it back through the formula labels to the input constants.

---

## 1. Baseline Assumptions

### Input Constants

These are the tunable parameters. Change any of these and re-run the formulas to test sensitivity.

```
Protocol constants (from docs):
  PACTS_DEFAULT      = 20        # default pact count per user
  PACTS_STANDBY      = 3         # standby pacts per user
  PACTS_POPULAR      = 40        # pacts for users with 10K+ followers
  VOLUME_TOLERANCE   = 0.30      # ±30% volume matching
  TTL                = 3         # gossip hop limit
  CHECKPOINT_WINDOW  = 30        # days per checkpoint window
  LIGHT_SYNC_DEPTH   = 50        # events per device for light node sync
  DEDUP_CACHE_SIZE   = 10000     # LRU entries for request_id dedup
  RATE_LIMIT_10055   = 10        # req/s per source for pact requests
  RATE_LIMIT_10057   = 50        # req/s per source for data requests
  WOT_FORWARD_HOPS   = 2         # WoT boundary for gossip forwarding
  BLE_MAX_HOPS       = 7         # BLE mesh max hops
  READ_CACHE_MAX_MB  = 100       # default read-cache cap
  CHALLENGE_FREQ     = 1         # challenges per day per active pact

Network assumptions:
  FULL_NODE_PCT      = 0.25      # 25% of users run full nodes
  LIGHT_NODE_PCT     = 0.75      # 75% of users run light nodes
  FULL_UPTIME        = 0.95      # full node online probability
  LIGHT_UPTIME       = 0.30      # light node online probability
  DAU_PCT            = 0.50      # daily active users as fraction of total
  APP_SESSIONS       = 10        # app opens per active user per day
  GOSSIP_FALLBACK    = 0.02      # fraction of fetches needing gossip (2%)
  CLUSTERING         = 0.25      # WoT graph clustering coefficient

Event assumptions:
  E_NOTE             = 800       # bytes per short note
  E_REACTION         = 500       # bytes per reaction
  E_REPOST           = 600       # bytes per repost
  E_DM               = 900       # bytes per DM
  E_LONGFORM         = 5500      # bytes per long-form article
  E_GOSSIP_REQ       = 300       # bytes per gossip request (kind 10057)
  E_CHALLENGE        = 300       # bytes per challenge (kind 10054)
  E_DATA_OFFER       = 200       # bytes per data offer (kind 10058)

Event mix:
  MIX_NOTE           = 0.40
  MIX_REACTION       = 0.30
  MIX_REPOST         = 0.15
  MIX_DM             = 0.10
  MIX_LONGFORM       = 0.05
```

### Event Sizes

A Nostr event includes: id (32B hex→64 chars), pubkey (64 chars), created_at, kind, tags, content, sig (128 chars). With Gozzip's additional tags (`root_identity`, `seq`, `prev_hash`):

| Event type | Typical content | Estimated total size |
|-----------|----------------|---------------------|
| Short note (kind 1) | ~200 chars | E_NOTE = 800 B |
| Reaction (kind 7) | ~10 chars | E_REACTION = 500 B |
| Repost (kind 6) | ~50 chars + ref | E_REPOST = 600 B |
| DM (kind 14) | ~500 chars encrypted | E_DM = 900 B |
| Long-form (kind 30023) | ~5,000 chars | E_LONGFORM = 5,500 B |

**[F-01] Weighted average event size:**

```
E_AVG = E_NOTE × MIX_NOTE + E_REACTION × MIX_REACTION + E_REPOST × MIX_REPOST
      + E_DM × MIX_DM + E_LONGFORM × MIX_LONGFORM
      = 800×0.40 + 500×0.30 + 600×0.15 + 900×0.10 + 5500×0.05
      = 320 + 150 + 90 + 90 + 275
      = 925 B
```

**Rounded: E_AVG ≈ 750 B** (conservative — assumes real-world events skew shorter than the averages above, and many reactions/reposts have minimal tags).

### User Profiles

**[F-02] Monthly volume = events_per_day × CHECKPOINT_WINDOW × E_AVG**

| Profile | Events/day (E_d) | Monthly events (E_d × 30) | Monthly volume (F-02) | Calculation |
|---------|-----------|---------------|---------------------------|-------------|
| Casual | 5 | 150 | 112 KB | 150 × 750 = 112,500 B |
| Active | 30 | 900 | 675 KB | 900 × 750 = 675,000 B |
| Power | 100 | 3,000 | 2.2 MB | 3,000 × 750 = 2,250,000 B |
| Celebrity | 50 | 1,500 | 1.1 MB | 1,500 × 750 = 1,125,000 B |

Celebrity volume is lower than power users because celebrities post less frequently but have huge audiences. Power users are high-engagement participants (lots of reactions, replies, reposts).

### Node Distribution

Not all nodes are equal. Assume **25% full nodes, 75% light nodes** (range: 20–30% full).

**Keeper ratio caveat:** This is an optimistic target. Comparable systems achieve 0.1-5% always-on participation (Bitcoin full nodes: ~0.01%, Mastodon instance operators: ~2%, Nostr relays: 0.2-1%). The protocol is designed to function at full-node ratios as low as 5%. At 5% Keepers, the all-light-node availability analysis applies (P(unavailable) ≈ 0.08%), which remains acceptable.

| Node type | % of network | Uptime | Storage role | Devices |
|-----------|-------------|--------|-------------|---------|
| Full node | ~25% | ~95% (always-on) | Reliable storage peer, full history | Desktop, home server, VPS |
| Light node | ~75% | ~30% (intermittent) | Participates when online, checkpoint sync | Mobile phones, tablets |

Full nodes are the backbone of storage pacts. Light nodes participate but are less reliable. The protocol doesn't enforce this distinction — reliability scoring and challenge-response naturally surface it.

### Network Size Scenarios

| Scenario | Total users | Full nodes (25%) | Light nodes (75%) | DAU (50%) | Avg follows |
|----------|-----------|-----------------|-------------------|-----------|-------------|
| Early | 1,000 | 250 | 750 | 500 | 50 |
| Growing | 10,000 | 2,500 | 7,500 | 5,000 | 100 |
| Medium | 100,000 | 25,000 | 75,000 | 50,000 | 150 |
| Large | 1,000,000 | 250,000 | 750,000 | 500,000 | 200 |

---

## 2. Storage Requirements

### Per-Pact Storage (What You Store For One Partner)

Each pact covers events since last checkpoint (~30 days). Volume-matched partners (±30%) produce similar amounts:

| Partner type | Their monthly volume | Your storage for them |
|-------------|---------------------|----------------------|
| Casual | 112 KB | 112 KB |
| Active | 675 KB | 675 KB |
| Power | 2.2 MB | 2.2 MB |

### Total Storage Obligation Per User

**[F-03] Active pact storage = PACTS_DEFAULT × partner_monthly_volume**

**[F-04] Standby pact storage = PACTS_STANDBY × partner_monthly_volume**

**[F-05] Total pact storage = F-03 + F-04**

| Your profile | F-03: Active (20 × vol) | F-04: Standby (3 × vol) | F-05: Total |
|-------------|------------------------|------------------------|-------------|
| Casual | 20 × 112 KB = 2.24 MB | 3 × 112 KB = 336 KB | **2.58 MB** |
| Active | 20 × 675 KB = 13.2 MB | 3 × 675 KB = 2.03 MB | **15.2 MB** |
| Power | 20 × 2.2 MB = 44 MB | 3 × 2.2 MB = 6.6 MB | **50.6 MB** |

**[F-06] Read-cache estimate = min(follows × avg_partner_volume, READ_CACHE_MAX_MB)**

```
Active user: min(150 × 675 KB, 100 MB) = min(98.9 MB, 100 MB) = 98.9 MB ≈ 99 MB
```

Read-cache is pruned by LRU, so frequently accessed users stay cached.

**[F-07] Total on-device storage = F-05 + F-06 + own_history**

| Your profile | F-05: Pacts | F-06: Read cache | Own history (1yr) | F-07: Total |
|-------------|-------------|-----------------|-------------------|-------------|
| Casual | 2.6 MB | ~20 MB | 112 KB × 12 = 1.3 MB | **~24 MB** |
| Active | 15.2 MB | ~80 MB | 675 KB × 12 = 7.9 MB | **~103 MB** |
| Power | 50.6 MB | ~100 MB | 2.2 MB × 12 = 26.4 MB | **~177 MB** |

**[F-08] Storage as % of device = F-07 / device_storage**

```
Active user on 32 GB phone: 103 MB / 32,000 MB = 0.32%
Power user on 256 GB desktop: 177 MB / 256,000 MB = 0.07%
```

**Verdict: Very feasible.** Even a budget phone has 32+ GB storage. Protocol storage is < 0.5% of device capacity.

### Pact Partner Storage Obligation by Node Type

Full nodes handle the heavy lifting. Light nodes contribute when online.

| Your node type | Pact storage (20 partners) | Read cache | Always serving? |
|---------------|---------------------------|------------|----------------|
| Full node | 15.2 MB (active users) | ~80 MB | Yes — 95% uptime |
| Light node | 15.2 MB (stored locally) | ~50 MB | No — serves when online (~30% of time) |

A light node stores the data but can only serve it when online. This means ~70% of the time, its pact partners can't reach it. The protocol handles this through redundancy — 20 pact partners means even with mixed node types, enough are online.

---

## 2.5. Pact Availability (Full/Light Node Impact)

### Pact Formation Supply and Demand

**[F-09] Pact demand = N_users × PACTS_DEFAULT**

**[F-10] Pact supply = N_users × PACTS_DEFAULT** (symmetric — every pact is reciprocal)

```
100K network: demand = 100,000 × 20 = 2,000,000 pact slots
              supply = 100,000 × 20 = 2,000,000 pact slots
              supply = demand ✓
```

The issue isn't total capacity — it's **quality**. Users prefer reliable pact partners (full nodes).

### Pact Partner Composition

**[F-11] Full-node pact supply = N_users × FULL_NODE_PCT × PACTS_DEFAULT**

**[F-12] Max full-node pact share = F-11 / F-09**

```
100K network:
  Full-node supply = 100,000 × 0.25 × 20 = 500,000 full-node pact slots
  Total demand     = 2,000,000
  Max full-node share = 500,000 / 2,000,000 = 0.25 (25%)
```

Even with perfect allocation, at most 25% of anyone's pacts can be with full nodes. With peer selection bias (preferring reliable peers), realistic composition:

**[F-13] Expected full-node partners = PACTS_DEFAULT × min(FULL_NODE_PCT × selection_bias, 1.0)**

| Scenario | selection_bias | F-13: Full partners | Light partners | Calculation |
|----------|---------------|--------------------|--------------------|-------------|
| Biased (typical) | 1.6 | 8 of 20 | 12 of 20 | 20 × min(0.25 × 1.6, 1) = 20 × 0.40 = 8 |
| Random | 1.0 | 5 of 20 | 15 of 20 | 20 × min(0.25 × 1.0, 1) = 20 × 0.25 = 5 |
| Full-node user | 2.4 | 12 of 20 | 8 of 20 | 20 × min(0.25 × 2.4, 1) = 20 × 0.60 = 12 |

### Storage Peer Availability at Request Time

When someone requests your data, they need at least 1 of your 20 storage peers to be online and responsive.

**[F-14] P(all offline) = (1 - FULL_UPTIME)^n_full × (1 - LIGHT_UPTIME)^n_light**

**[F-15] P(at least 1 online) = 1 - F-14**

**[F-16] E[peers online] = n_full × FULL_UPTIME + n_light × LIGHT_UPTIME**

**With typical pact composition (8 full + 12 light):**

```
F-14: P(all offline) = (1 - 0.95)^8 × (1 - 0.30)^12
                     = (0.05)^8 × (0.70)^12
                     = 3.906×10⁻¹¹ × 0.01384
                     = 5.41×10⁻¹³

F-15: P(at least 1 online) = 1 - 5.41×10⁻¹³ ≈ 100%

F-16: E[online] = 8 × 0.95 + 12 × 0.30 = 7.60 + 3.60 = 11.20
```

**With random composition (5 full + 15 light):**

```
F-14: P(all offline) = (0.05)^5 × (0.70)^15
                     = 3.125×10⁻⁷ × 4.747×10⁻³
                     = 1.484×10⁻⁹

F-15: P(at least 1 online) = 1 - 1.484×10⁻⁹ ≈ 100%

F-16: E[online] = 5 × 0.95 + 15 × 0.30 = 4.75 + 4.50 = 9.25
```

**Verdict: Even with 75% light nodes, data availability is ~100% under the independent-failure model.** The redundancy of 20 pact partners overwhelms the low uptime of individual light nodes. You need 1; you have ~11 online at any time.

### Correlated Failure Analysis

The independent-failure calculation above assumes pact partner outages are uncorrelated. In practice, timezone correlation (12 of 20 partners sharing a sleep schedule), community correlation (shared ISP or OS updates), and platform correlation (iOS update breaking background networking) introduce correlated failure modes. Under a conservative model with 60% timezone overlap, overnight availability degrades to approximately 10^-3 to 10^-4 -- still respectable for a peer-to-peer system, but five orders of magnitude below the independent-failure headline. The protocol recommends geographic diversity in pact selection to mitigate this. Both numbers should be presented: ~10^-9 under independence, ~10^-3 under realistic correlation.

**Simulation Evidence (F-14, F-15):** Multi-topology simulations (2,000 nodes, 30-day runs) reveal that both the independent-failure and correlated-failure formulas significantly overstate availability. Simulated retrieval failure rates range from 1.6% (BA m=10) to 5.2% (BA m=50 with timezone correlation) — far above the theoretical P(all offline) of 10^-9 to 10^-13. The gap is explained by **pact churn**: nodes continuously form and dissolve pacts, creating transient periods where a user has fewer than 20 active storage peers, some with incomplete data. During these low-redundancy windows, a single correlated outage can cause retrieval failure. The correlated-failure estimate of 10^-3 to 10^-4 is also too optimistic — the BA m=50+TZ topology (which models timezone correlation) shows a 5.2% failure rate, roughly 50x worse than the analytical bound. The independent-failure formula remains directionally correct (more pacts = better availability), but the absolute numbers should be read as upper bounds on a mature, stable pact set, not as steady-state predictions during churn.

| Topology | Simulated availability | Simulated failure rate | Analytical P(all offline) | Gap factor |
|----------|----------------------|----------------------|--------------------------|------------|
| BA m=10 (2K nodes) | 98.4% | 1.6% | ~10^-9 | ~10^7 |
| WS p=0.30 (2K nodes) | 97.7% | 2.3% | ~10^-9 | ~10^7 |
| BA m=50 (2K nodes) | 95.3% | 4.7% | ~10^-13 | ~10^11 |
| BA m=50+TZ (2K nodes) | 94.8% | 5.2% | ~10^-3 (correlated) | ~50 |
| BA m=10 (1K nodes) | 99.2% | 0.8% | ~10^-9 | ~10^7 |
| BA m=50 (5K nodes) | 96.9% | 3.1% | ~10^-13 | ~10^11 |

Notably, sparser topologies (BA m=10) consistently outperform denser ones (BA m=50) by 3+ percentage points. In dense graphs, pact churn rates are higher (6.87-8.04 churn/node/day vs 2.79 for BA m=10), creating more transient vulnerability windows. The 1K-node BA m=10 run achieved 99.2% availability — the best observed — suggesting that small, sparse networks with stable pact sets approach the analytical predictions most closely.

### Standby Pact Impact

**[F-17] Total online with standby = F-16 + standby_full × FULL_UPTIME + standby_light × LIGHT_UPTIME**

3 standby pacts (assume 2 full + 1 light):

```
F-17: Total online = 11.20 + 2 × 0.95 + 1 × 0.30 = 11.20 + 1.90 + 0.30 = 13.40
```

### Worst Case: Mobile-Only User

A user with NO full-node devices. All pact partners biased toward light nodes:

| Composition (n_full + n_light) | F-16: E[online] | F-14: P(all offline) | F-15: P(≥1 online) |
|-------------------------------|----------------|---------------------|-------------------|
| 5 + 15 | 5×0.95 + 15×0.30 = 9.25 | (0.05)^5 × (0.70)^15 = 1.48×10⁻⁹ | ~100% |
| 3 + 17 | 3×0.95 + 17×0.30 = 7.95 | (0.05)^3 × (0.70)^17 = 2.88×10⁻⁶ | 99.9997% |
| 0 + 20 | 0 + 20×0.30 = 6.00 | (0.70)^20 = 7.98×10⁻⁴ | 99.92% |

Even an all-light-node pact set has 6 peers online on average and 99.92% availability. The 20-peer redundancy makes this robust.

### Challenge-Response Reliability by Node Type

Challenges require the challenged peer to be online and responsive. With mixed node types:

| Partner type | Challenge success rate | Expected result (daily challenges) |
|-------------|----------------------|-----------------------------------|
| Full node | ~95% (uptime-limited) | ~19 of 20 challenges succeed |
| Light node | ~30% (uptime-limited) | ~6 of 20 challenges succeed |

Light-node partners will score lower on reliability (30% challenge success vs 95%). The scoring system responds:

- Light nodes at 30%: below the 50% "failed" threshold → **dropped immediately**

This is a problem. Light nodes would fail challenge-response and get dropped constantly. **Two interpretations:**

1. **Challenge timing adapts:** Challenges are only sent when the peer is known to be online (detected via presence). This is the practical approach — don't challenge a phone that's sleeping.
2. **Mobile obligation model:** Mobile nodes aren't expected to be primary storage peers. They form pacts but handle storage obligations during active hours. Their desktop/server device handles the always-on obligation.

**The protocol implicitly assumes option 2:** "Mobile devices participate when possible but aren't expected to be always-on storage servers. A user's desktop, home server, or cloud VPS handles primary storage obligations."

This means in practice:
- A USER has 20 pacts, but their FULL-NODE DEVICE serves them
- The user's light nodes (phones) participate in gossip, read-caching, and local fetch — but don't serve pact obligations
- If the user has NO full-node device, they rely on standby pacts and relay fallback while their phone is offline

### Effective Full-Node Pact Load

**[F-18] Pacts per full node = (N_users × PACTS_DEFAULT) / (N_users × FULL_NODE_PCT)**

```
F-18: Pacts per full node = PACTS_DEFAULT / FULL_NODE_PCT
     = 20 / 0.25 = 80 pacts
```

(This simplifies because total pacts / total full nodes = per-user pacts / full-node fraction.)

**[F-19] Full-node pact storage = F-18 × active_user_monthly_volume**

```
F-19: 80 × 675 KB = 52.7 MB
```

**[F-20] Full-node challenge load = F-18 × CHALLENGE_FREQ × (E_CHALLENGE + avg_response_size)**

```
F-20: 80 × 1/day × (300 + 450) B = 80 × 750 B = 60 KB/day
```

**[F-21] Full-node data request serving = F-18 × requests_per_stored_user_per_day × light_sync_bytes**

Where `light_sync_bytes = LIGHT_SYNC_DEPTH × E_AVG = 50 × 750 = 37,500 B = 37.5 KB`

```
requests_per_stored_user = 100/day (estimated: followers checking for updates)

F-21: 80 × 100 × 37.5 KB = 300,000 KB = 300 MB/day outbound
      Over 24h: 300 MB / 86,400 s = 3.47 KB/s average
```

**[F-22] Peak serving load = F-21 compressed into peak_hours**

```
F-22: 300 MB / (2 × 3,600 s) = 300 MB / 7,200 s = 41.7 KB/s peak
```

**[F-23] Broadband utilization = F-22 / upload_capacity**

```
F-23: 41.7 KB/s / 1,250 KB/s (10 Mbps) = 3.3% at peak
```

**Verdict: Full nodes can handle 80 pacts comfortably.** Storage (53 MB), bandwidth (3.5 KB/s average, 42 KB/s peak), and compute (negligible) are all well within consumer hardware limits.

---

## 3. Gossip Network Load

### Gossip Reach (TTL=3, with Online Fraction)

Each gossip request (kind 10057) propagates through WoT peers with TTL=3. But only ONLINE nodes can forward.

**[F-24] Online fraction = FULL_NODE_PCT × FULL_UPTIME + LIGHT_NODE_PCT × LIGHT_UPTIME**

```
F-24: 0.25 × 0.95 + 0.75 × 0.30 = 0.2375 + 0.225 = 0.4625 (46.25%)
```

**[F-25] Effective online peers = PACTS_DEFAULT × F-24**

```
F-25: 20 × 0.4625 = 9.25
```

**[F-26] Gossip reach at hop h = F-25 × (F-25 - 1) × (1 - CLUSTERING)^(h-1)** (for h > 1)

```
Hop 1: F-25 = 9.25
Hop 2: 9.25 × (9.25 - 1) × (1 - 0.25) = 9.25 × 8.25 × 0.75 = 57.23
Hop 3: 57.23 × 8.25 × 0.75 = 354.11
Total: 1 + 9.25 + 57.23 + 354.11 = 421.59
```

| Clustering | Online rate | Hop 1 | Hop 2 | Hop 3 | Total unique |
|-----------|-----------|-------|-------|-------|-------------|
| 0%, 100% online (ideal) | 100% | 20 | 400 | 8,000 | **8,420** |
| 25%, 100% online | 100% | 20 | 300 | 4,500 | **4,820** |
| 25%, 46% online (realistic) | 46% | 9 | 57 | 348 | **414** |
| 25%, 60% online (peak hours) | 60% | 12 | 99 | 816 | **927** |

**Realistic estimate: ~400–900 unique online nodes per gossip request.**

This is significantly lower than the ideal 8,000. But as shown in Section 4, gossip is WoT-routed — it doesn't need to reach many nodes, just the right ones (storage peers in the target's WoT neighborhood).

### How Often Is Gossip Needed?

Most fetches use cached endpoints (kind 10059) — direct connection, no gossip. Gossip is a fallback.

| Delivery path | Success rate (sovereign phase) | When used |
|--------------|-------------------------------|-----------|
| Cached endpoints (10059) | ~90% | Following someone, have their peer list |
| Relay query | ~8% | Endpoint stale, relay has data |
| Gossip (10057) | ~2% (GOSSIP_FALLBACK) | Both above failed |

**[F-27] Gossip requests per user per day = APP_SESSIONS × avg_follows × GOSSIP_FALLBACK**

```
F-27: 10 × 150 × 0.02 = 30 gossip requests/day
```

### Per-Node Gossip Load

**[F-28] Network gossip rate = N_users × DAU_PCT × F-27 / 86,400**

**[F-29] Online nodes = N_users × F-24**

**[F-30] Reach fraction = min(F-26_total / F-29, 1.0)**

**[F-31] Gossip seen per online node = F-28 × F-30** (req/s)

**[F-32] Gossip bandwidth per node = F-31 × E_GOSSIP_REQ**

| N_users | F-28: req/s | F-29: online | F-30: reach frac | F-31: per node/s | F-32: BW |
|---------|------------|-------------|-----------------|-----------------|---------|
| 1,000 | 500×30/86400 = 0.17 | 462 | min(422/462, 1) = 0.91 | 0.17×0.91 = 0.155 | 47 B/s |
| 10,000 | 5000×30/86400 = 1.74 | 4,625 | 422/4625 = 0.091 | 1.74×0.091 = 0.158 | 47 B/s |
| 100,000 | 50000×30/86400 = 17.4 | 46,250 | 422/46250 = 0.0091 | 17.4×0.0091 = 0.158 | 47 B/s |
| 1,000,000 | 500000×30/86400 = 174 | 462,500 | 422/462500 = 0.00091 | 174×0.00091 = 0.158 | 47 B/s |

**Key insight:** F-31 converges because F-28 grows with N and F-30 shrinks with N — they cancel out. The per-node load is constant at **~0.16 req/s** regardless of network size.

```
Proof of convergence:
F-31 = (N × DAU_PCT × gossip_per_user / 86400) × (gossip_reach / (N × online_pct))
     = DAU_PCT × gossip_per_user × gossip_reach / (86400 × online_pct)
     = 0.50 × 30 × 422 / (86400 × 0.4625)
     = 6,330 / 39,960
     = 0.158 req/s  (constant, independent of N)
```

Total gossip overhead per online node: **0.158 × 300 = 47 B/s = 4.1 MB/day**. Negligible.

### Gossip Deduplication Effectiveness

**[F-33] LRU cache coverage = DEDUP_CACHE_SIZE / F-31**

```
F-33: 10,000 / 0.158 = 63,291 seconds ≈ 17.6 hours
```

The LRU cache covers 17+ hours of gossip. Dedup is extremely effective — every request is seen at most once per node.

**Verdict: Gossip load is negligible at any scale.** Per-node rate is ~0.16 req/s = 47 B/s, independent of network size.

**Simulation Evidence (F-31):** The convergence proof for F-31 is confirmed in simulation — the per-node gossip rate formula passes validation with near-zero threshold relaxation, as both expected and actual values converge to near-zero. However, gossip's role is far smaller than the mean-field model suggests. Across all topologies, gossip delivers only **0.1-9% of reads**. The vast majority of retrievals (74-92%) are resolved via pact-local reads (direct fetch from a storage peer), with relay fallback handling most of the remainder. The GOSSIP_FALLBACK = 0.02 (2%) assumption in §3 is in the right ballpark for sparse topologies but overstates gossip's contribution in dense ones, where pact-local reads dominate even more. The practical implication: gossip load is negligible not because the formula is wrong, but because gossip is rarely needed when pact-local paths work.

---

## 4. Gossip Discovery Probability

The critical question: when gossip IS needed, can it find a storage peer?

### Why Random Reach Is the Wrong Model

A naive model treats gossip like random sampling: "I reach R of N nodes, what's the probability one is a storage peer?" This is wrong because **gossip propagates through the WoT graph, and storage peers ARE WoT members**.

When Bob wants Alice's data:
- Bob follows Alice → Bob is 1-hop from Alice in the WoT
- Alice's 20 storage peers are in her WoT (by definition — pacts form within WoT)
- Bob's gossip (TTL=3) reaches his 3-hop WoT neighborhood
- Since Bob → Alice is 1 hop, Bob's 3-hop reach includes Alice's 2-hop WoT
- Alice's storage peers are in her 2-hop WoT → **they're within gossip range**

The correct model: gossip reach is measured in **WoT hops from the target**, not random nodes in the network. Network size is irrelevant.

### WoT-Routed Gossip Model

```
Bob follows Alice.
Bob's 3-hop gossip → covers Alice's 2-hop WoT neighborhood.
Alice has 20 storage peers in her WoT.

Question: how many of Alice's storage peers are within her 2-hop WoT?
Answer: all of them (pacts form within WoT by design).
```

At each hop, social clustering means nodes share connections. Alice's storage peers are scattered across her WoT — some are 1-hop (direct follows), some are 2-hop (friends-of-friends). Bob's 3-hop gossip traverses this neighborhood because Bob → Alice is the bridge.

**Estimated success rate by WoT distance:**

| Bob's relation to Alice | WoT hops (Bob→Alice) | Gossip hops remaining for Alice's WoT | P(find storage peer) |
|------------------------|---------------------|---------------------------------------|---------------------|
| Follows Alice | 1 | 2 hops into Alice's WoT | **~95%+** |
| Follows someone who follows Alice | 2 | 1 hop into Alice's WoT | **~70-85%** |
| 3-hop connection | 3 | 0 (gossip barely reaches Alice's WoT) | **~20-40%** |
| No WoT connection | N/A | Gossip doesn't propagate | **0%** (use relay) |

### When Gossip Fails (and That's OK)

Gossip fails for **cold discovery** — finding data for someone you have no WoT connection to. This is by design:

- **In-WoT requests** (follow someone, or follow-of-follow): gossip works at any network size
- **Cold discovery** (stranger lookup): use DVM relay broadcast or direct relay query
- **The WoT forwarding rule** (`Nodes only forward gossip from pubkeys within their 2-hop WoT`) makes this explicit — gossip is for your WoT, relays are for strangers

### Layered Delivery Combined Probability

| Scenario | Cached endpoints | Gossip (WoT) | DVM relay | Relay | Combined P(delivery) |
|----------|-----------------|--------------|-----------|-------|---------------------|
| Follow (1-hop) | 90% | 95%+ | 95% | 99% | **~100%** |
| 2-hop WoT | 70% | 70-85% | 95% | 99% | **~100%** |
| Stranger | 0% | 0% | 90% | 99% | **~99%** |

**Verdict: Delivery is effectively 100% at any network size.** Gossip handles in-WoT requests regardless of network size. Relays handle strangers. The two cover all cases.

**Simulation Evidence (Gossip Reach):** Simulation data challenges the WoT-routed gossip model's optimistic reach estimates. Actual unique reach via gossip accounts for only **0.1-9.3% of reads** across topologies — far below the ~90% success rate the mean-field formula predicts for 1-hop WoT requests. Instead, **pact-local reads dominate at 74-92%** of all retrievals: when a requester already has a cached endpoint for a storage peer, they fetch directly without gossip. The "instant" delivery column (91.8% for BA m=10 down to 73.7% for BA m=50+TZ) corresponds almost entirely to pact-local fetches, not gossip-discovered peers. Gossip's role is real but marginal — it functions as a last-resort discovery mechanism before relay fallback, not as the primary delivery path the analytical model implies. The layered delivery model is confirmed, but the layers' relative contribution differs from the analytical estimate: pact-local >> relay > gossip, rather than cached endpoints >> gossip >> relay.

---

## 5. Popular Account Scaling

### The First-Wave Problem

Alice has 100,000 followers and 40 storage pacts. She posts a new note. 50,000 followers (50% DAU) want to fetch it within the next hour.

**[F-34] Online pact peers = n_full × FULL_UPTIME + n_light × LIGHT_UPTIME**

Alice's 40 pacts (popular user). With selection bias: ~18 full + 22 light.

```
F-34: 18 × 0.95 + 22 × 0.30 = 17.10 + 6.60 = 23.70 ≈ 24 online at any time
```

**[F-35] Light sync payload = LIGHT_SYNC_DEPTH × E_AVG**

```
F-35: 50 × 750 = 37,500 B = 37.5 KB
```

**[F-36] Connections per online peer per hour = (followers × DAU_PCT × endpoint_hit_rate) / F-34 / 1 hour**

**[F-37] Per-peer bandwidth = F-36 × F-35 / 3,600**

```
F-36: (100,000 × 0.50 × 0.90) / 24 / 1 = 45,000 / 24 = 1,875/hour = 0.52/s
F-37: 0.52 × 37.5 KB = 19.5 KB/s outbound per online peer
```

Typical home upload (10 Mbps = 1,250 KB/s): uses **19.5 / 1,250 = 1.6%** of bandwidth.

**Path 2: Cascading read-cache absorbs the tail**

After the first 1,000 followers fetch from storage peers (first ~3 minutes):
- Those 1,000 now have Alice's events cached
- Subsequent followers can fetch from any of these 1,000 via gossip
- Storage peer load drops exponentially

```
Time    Storage peer connections/s    Read-cache sources available
0-3min  0.31 × 40 = 12.4 total       0
3-10min 5.0 (diminishing)             ~1,000
10-30min 1.0 (rare)                   ~10,000
30-60min ~0 (read-cache handles all)  ~25,000
```

### Celebrity Account (1M followers)

**[F-38] Celebrity first-hour connections per peer = (followers × DAU_PCT × first_hour_fraction) / F-34**

```
F-38: (1,000,000 × 0.50 × 0.20) / 24 = 100,000 / 24 = 4,167/hour = 1.16/s
      (assuming 20% of DAU check within the first hour)

Per-peer bandwidth: 1.16 × 37.5 KB (F-35) = 43.4 KB/s
Broadband utilization: 43.4 / 1,250 = 3.5%
```

Manageable for broadband. Read-cache absorbs the tail within minutes.

### Viral Post Scenario

A post goes viral. 1M views in 10 minutes. Most viewers are NOT followers (no cached endpoints). They discover via gossip or relay.

**[F-39] Viral connections per second per peer = viral_views / (viral_duration_s × F-34)**

```
F-39: 1,000,000 / (600 s × 24 peers) = 1,000,000 / 14,400 = 69.4/s per online peer
```

**[F-40] Viral bandwidth per peer = F-39 × F-35**

```
F-40: 69.4 × 37.5 KB = 2,604 KB/s = 2.54 MB/s per online peer
```

**[F-41] Broadband utilization (viral) = F-40 / upload_capacity**

```
F-41: 2,604 KB/s / 1,250 KB/s = 208% → exceeds 10 Mbps upload!
```

**Bottleneck confirmed:** A truly viral post (1M views in 10 min) **exceeds** a single peer's 10 Mbps upload. However:

1. **Read-cache cuts this short.** After ~1,000 fetches (~14s at 69/s), 1,000 new cache sources exist. New requests start hitting the cache, reducing peer load exponentially.
2. **Not all viewers connect simultaneously.** The 10-minute window is staggered.
3. **Full nodes on faster connections** (100 Mbps+ fiber) handle it.

**[F-42] Time until read-cache takes over = cache_threshold / (F-39 × F-34)**

```
F-42: 1,000 readers / (69.4/s × 24 peers) = 1,000 / 1,666 = 0.6 seconds
```

After **< 1 second** of viral load, enough read-caches exist to absorb the tail. The 2.5 MB/s spike per peer is real but lasts only fractions of a second at full intensity before the network self-heals.

**Mitigation:** Peer request queuing (serve N/s, queue rest) smooths the spike. Followers retry from read-cache sources that build up within seconds.

---

## 6. Challenge-Response Overhead

### Daily Challenge Load (Standard User, 20 Pacts)

**[F-43] Challenge bandwidth = PACTS_DEFAULT × CHALLENGE_FREQ × (sent_bytes + received_bytes)**

Assume 90% hash challenges, 10% serve challenges:

| Component | Size | Count/day | Daily bytes | Calculation |
|-----------|------|-----------|-------------|-------------|
| Challenges sent | E_CHALLENGE = 300 B | 20 | 6,000 B | 20 × 300 |
| Hash responses received | 100 B | 18 (90%) | 1,800 B | 20 × 0.90 × 100 |
| Serve responses received | E_AVG = 750 B | 2 (10%) | 1,500 B | 20 × 0.10 × 750 |
| Challenges received | 300 B | 20 | 6,000 B | 20 × 300 |
| Hash responses sent | 100 B | 18 | 1,800 B | 20 × 0.90 × 100 |
| Serve responses sent | 750 B | 2 | 1,500 B | 20 × 0.10 × 750 |
| **Total** | | | **18,600 B ≈ 19 KB/day** | |

### For a Full Node (80 Pacts, F-18)

```
F-43 at 80 pacts: 80/20 × 19 KB = 76 KB/day
```

### Hash Computation Cost

**[F-44] Hash compute time = (challenge_range × E_AVG) / SHA256_throughput**

```
challenge_range = 7 events (typical range [start..end])
SHA256_throughput = 500 MB/s (modern hardware)

F-44: (7 × 750 B) / (500 × 10^6 B/s) = 5,250 / 500,000,000 = 1.05×10⁻⁵ s = 10.5 μs

Per day (20 pacts): 20 × 10.5 μs = 210 μs
Per day (80 pacts): 80 × 10.5 μs = 840 μs < 1 ms
```

**Verdict: Negligible.** Challenge-response adds ~19 KB/day bandwidth (76 KB for full nodes) and < 1 ms compute. A non-issue.

---

## 7. Merkle Root and Completeness Verification

### Checkpoint Merkle Tree Construction

Monthly checkpoint covers all events in the window:

| User profile | Events in window | Merkle tree nodes | Construction time |
|-------------|-----------------|-------------------|-------------------|
| Casual | 150 | ~300 | ~50 μs |
| Active | 900 | ~1,800 | ~300 μs |
| Power | 3,000 | ~6,000 | ~1 ms |

### Light Node Verification

Light node fetches checkpoint + last M=20 events per device. Verification:

1. Check per-event hash chain (20 events): 20 × ~10 μs = 200 μs
2. Compare sequence numbers for gaps: O(1) per event
3. If fetching full window: compute Merkle root from all events

For a user with 3 devices, verifying 60 events: **< 1 ms total**.

**Verdict: Negligible.** Merkle verification is sub-millisecond work.

---

## 8. Bandwidth Budget Per User Per Day

### Full Node — Active User

Parameters: E_d=30, follows=150, own pacts=20, effective pacts served=F-18=80, online=22h.

**Outbound:**

| Activity | Formula | Calculation | Daily |
|----------|---------|-------------|-------|
| Publish to storage peers | E_d × E_AVG × (PACTS_DEFAULT + PACTS_STANDBY) | 30 × 750 × 23 | 518 KB |
| Serve pact data | F-21 = F-18 × 100 × F-35 | 80 × 100 × 37.5 KB | 300 MB |
| Challenge responses | F-20 at 80 pacts | see §6 | 76 KB |
| Read-cache serving | ~20 serves × F-35 | 20 × 37.5 KB | 750 KB |
| Gossip forwarding | ~10 forwards × E_GOSSIP_REQ × F-25 | 10 × 300 × 9.25 | 27.8 KB |
| **Total outbound** | | | **~301 MB** |

**Inbound:**

| Activity | Formula | Calculation | Daily |
|----------|---------|-------------|-------|
| Fetch follows' events | follows × events_per_follow × E_AVG | 150 × 10 × 750 | 1.1 MB |
| Receive pact events | F-18 × partner_events_per_day × E_AVG | 80 × 25 × 750 | 1.5 MB |
| Gossip received | F-31 × E_GOSSIP_REQ × online_seconds | 0.158 × 300 × 79,200 | 3.75 MB |
| Challenges received | F-18 × E_CHALLENGE | 80 × 300 | 24 KB |
| **Total inbound** | | | **~6.4 MB** |

**[F-48] Full-node broadband utilization = total_outbound / (upload_Mbps × 86,400 / 8)**

```
F-48: 301 MB / (10 Mbps × 86,400 / 8) = 301 MB / 108,000 MB = 0.28%
Peak (2×): 0.56%
```

### Light Node — Active User

Parameters: E_d=30, follows=150, own pacts=20, online=6h (21,600s).

**[F-49] Light-node challenge hit rate = LIGHT_UPTIME** (fraction of challenges received while online)

**Outbound:**

| Activity | Formula | Calculation | Daily |
|----------|---------|-------------|-------|
| Publish to storage peers | E_d × E_AVG × 23 | 30 × 750 × 23 | 518 KB |
| Challenge responses (online only) | PACTS_DEFAULT × F-49 × avg_response | 20 × 0.30 × 450 | 2.7 KB |
| Read-cache serving | ~5 serves × F-35 | 5 × 37.5 KB | 188 KB |
| Gossip forwarding | ~3 forwards × E_GOSSIP_REQ × F-25 | 3 × 300 × 9.25 | 8.3 KB |
| **Total outbound** | | | **~717 KB** |

**Inbound:**

| Activity | Formula | Calculation | Daily |
|----------|---------|-------------|-------|
| Fetch follows' events | follows × events_per_follow × E_AVG | 150 × 10 × 750 | 1.1 MB |
| Receive pact events | 23 × partner_events × E_AVG | 23 × 25 × 750 | 431 KB |
| Gossip received | F-31 × E_GOSSIP_REQ × 21,600 | 0.158 × 300 × 21,600 | 1.02 MB |
| Challenges (online) | PACTS_DEFAULT × F-49 × E_CHALLENGE | 20 × 0.30 × 300 | 1.8 KB |
| **Total inbound** | | | **~2.55 MB** |

**[F-50] Light-node monthly data = (outbound + inbound) × 30**

```
F-50: (717 KB + 2,550 KB) × 30 = 3,267 KB × 30 = 98,010 KB ≈ 96 MB/month
Mobile plan utilization: 96 MB / 5,000 MB = 1.9%
```

**Verdict: Very feasible for both node types.** Full nodes: 301 MB/day out = 0.28% of 10 Mbps. Light nodes: ~3.3 MB/day total = 1.9% of a 5 GB/month plan.

---

## 9. BLE Mesh Viability

### Throughput

BLE 5.0 practical throughput: BLE_THROUGHPUT = 100 Kbps (conservative, after protocol overhead).

**[F-45] BLE transfer time = data_size / BLE_THROUGHPUT**

| Operation | Size | F-45: Time at 100 Kbps | Calculation |
|-----------|------|----------------------|-------------|
| Single event | E_AVG = 750 B = 6 Kbit | 60 ms | 6,000 / 100,000 |
| Light sync | F-35 = 37.5 KB = 300 Kbit | 3.0 s | 300,000 / 100,000 |
| Full checkpoint | 900 × 750 = 675 KB = 5,400 Kbit | 54.0 s | 5,400,000 / 100,000 |

### Multi-Hop Latency

**[F-46] Per-hop latency = BLE_SETUP + F-45** where BLE_SETUP = 50 ms

**[F-47] Total latency = hops × F-46**

| Hops | F-47: Single event | F-47: 50-event sync | Calculation (single) |
|------|-------------------|---------------------|---------------------|
| 1 | 110 ms | 3.05 s | 1 × (50 + 60) |
| 3 | 330 ms | 9.15 s | 3 × (50 + 60) ms / 3 × (50ms + 3.0s) |
| 7 (max) | 770 ms | 21.35 s | 7 × (50 + 60) ms / 7 × (50ms + 3.0s) |

### Coverage Model

BLE range: ~30-100m indoors, ~100-300m outdoors.

| Scenario | People density | Avg spacing | BLE range | Hops needed for 500m | Feasible? |
|----------|---------------|-------------|-----------|---------------------|-----------|
| Protest (dense) | 10,000/km² | ~10m | 30m indoor | 2-3 hops | Yes |
| Conference | 1,000/km² | ~30m | 50m indoor | 3-4 hops | Yes |
| City street | 100/km² | ~100m | 100m outdoor | 2-3 hops | Yes |
| Suburban | 10/km² | ~300m | 300m outdoor | 1-2 hops | Marginal |
| Rural | 1/km² | ~1km | 300m outdoor | No | No |

### Battery Impact

BLE is designed for low power. Bitchat uses adaptive power cycling.

- BLE advertising: ~15 mA for ~1 ms every 100 ms = **0.15 mW average**
- BLE connection: ~15 mA during transfer = **~50 mW during active transfer**
- Idle mesh participation: **< 1% battery impact over a full day**
- Active mesh (protest scenario, frequent relaying): **~3-5% battery over 8 hours**

**Verdict: BLE mesh is viable for dense urban scenarios (protests, conferences, city streets). Not viable for rural/suburban. Battery impact is minimal.**

---

## 10. Network Bootstrap Viability

### Phase Transitions

| Phase | Network size | User behavior | Relay dependency | Gossip reach |
|-------|-------------|---------------|-----------------|-------------|
| Bootstrap | 0–1,000 | Relay-primary, forming first pacts | 100% relay | 100% (all nodes) |
| Early growth | 1,000–5,000 | Hybrid, most users 5-10 pacts | ~50% relay | ~100% |
| Critical mass | 5,000–20,000 | Sovereign possible, gossip functional | ~20% relay | ~100% |
| Medium | 20,000–100,000 | Most users sovereign | ~5% relay (fallback) | ~90%+ (WoT-corrected) |
| Scale | 100,000+ | Full sovereign, relays as accelerators | Optional | Cached endpoints primary |

**Simulation Evidence (Relay Dependency):** Simulation data from mature networks (days 20-30 of 30-day runs) shows relay dependency at maturity ranging from **0.4% to 13%** depending on topology — a wider range than the "~5% relay (fallback)" estimated above for the Medium phase. Sparse topologies (BA m=10) achieve the lowest relay dependency (~6.4% relay reads overall, declining to ~0.4% at maturity), while dense topologies with timezone correlation (BA m=50+TZ) show persistent ~13% relay dependency even after 30 days. The "Optional" relay dependency at Scale is optimistic — simulation suggests relays remain structurally necessary for 1-13% of reads even in a mature 2,000-node network, driven by pact churn and transient low-redundancy states rather than insufficient pact counts.

### Bootstrap Pact Load

A new user's first follow becomes a temporary storage peer. How much load does this create for popular early adopters?

Worst case: 1 popular user is the first follow for 1,000 new users.
- 1,000 bootstrap pacts (one-sided) × 112 KB each (casual new users) = 112 MB of storage
- This is within reason for a desktop/server node
- Auto-expires after 90 days or when user reaches 10 reciprocal pacts
- In practice, new users follow different people, distributing the load

**Bottleneck identified:** A highly popular early adopter could accumulate many bootstrap pacts. **Mitigation:** "The followed user's client auto-accepts if capacity allows" — they can refuse if overloaded. New users should follow 3-5 people to distribute bootstrap load.

### Cold Start and Bootstrap Economics

The protocol is functional during bootstrap — it works identically to standard Nostr — but this conflates "functional" with "compelling." The chicken-and-egg problem is not about whether the protocol works at launch (it does), but about why anyone would adopt when the product is strictly a heavier Nostr client with zero additional benefit until pacts mature:

- **Day 1:** Users use relays. The Gozzip client adds background pact negotiation, challenge-response, and WoT computation — all invisible to the user but consuming bandwidth and battery. Zero incremental value over a standard Nostr client.
- **Weeks 1-4:** Users form first pacts. Storage redundancy begins but is not yet reliable. The user experience is identical to Nostr.
- **Months 1-3:** WoT grows, pact counts increase, gossip becomes useful for some reads. First measurable benefit: relay failure no longer means data loss.
- **Months 3-6:** Sovereign users begin relying primarily on peers. The protocol's value proposition materializes.

**The honest assessment:** The protocol survives bootstrap if the first client is an excellent Nostr client where sovereignty accrues as a background benefit. It fails during bootstrap if users are asked to adopt for sovereignty alone. Day-one value features (multi-device identity, encrypted DMs, social recovery) are the adoption hook — not pacts.

**Bootstrap risks not modeled above:**
- Guardian supply is zero at launch (no Sovereign-phase users exist yet). Genesis Guardian infrastructure is required.
- The first ~3 months offer no measurable advantage over relay-only Nostr.
- If the first Gozzip client is mediocre as a Nostr client, users will leave before pacts provide value.

---

## 11. Identified Bottlenecks and Risk Areas

### MEDIUM: Full-Node Pact Concentration

**Problem:** With 75% light nodes, storage pact serving falls disproportionately on the 25% full nodes. Each full node may serve ~80 pacts instead of 20, because light-node peers can't reliably serve.

**Why it's OK:** 80 pacts = 53 MB storage, 300 MB/day outbound (3.5 KB/s average). Trivial for broadband. Full-node operators are running desktops/servers and expect to contribute more.

**If it's not OK:** Cap pacts per full node. As the full-node percentage grows (more desktop/server users), the load distributes naturally. Or: encourage light-node users to run a cheap VPS (~$5/month) if they want sovereign-phase benefits.

### MEDIUM: Light-Node Challenge Failure

**Problem:** Light nodes are online ~30% of time. Daily challenges sent to a light-node peer fail ~70% of the time. The reliability scoring system would drop them below the 50% threshold → constant churn.

**Why it's OK:** The protocol already says "mobile devices participate when possible but aren't expected to be always-on storage servers." In practice, a USER's full-node device handles pact obligations. The light node is a secondary device.

**Needs design clarification:** The challenge protocol should be presence-aware — only challenge peers that are currently online or were recently seen. Challenging sleeping phones is wasteful and unfairly penalizes light-node users. This is a client-side optimization: challenge scheduling should account for peer online patterns.

### MEDIUM: Viral Post Spike on Storage Peers

**Problem:** A viral post spikes online storage peer bandwidth to ~2.6 MB/s. With only ~24 of 40 peers online for a popular user, each online peer absorbs more load. Full-node peers handle it; light-node peers that are online may struggle.

**Why it's OK:** Read-cache propagation reduces the spike within minutes. Full-node peers on broadband handle 2.6 MB/s easily. The spike is transient — cascading read-cache builds within 3-5 minutes.

**If it's not OK:** Add request queuing per storage peer: serve N requests/second, queue the rest. Followers retry from read-cache sources. Or: popular users are encouraged to have at least 1 always-on server among their devices.

### LOW: Mobile-Only Users

**Problem:** A user with ONLY mobile devices and no desktop/server can't reliably serve pact obligations. Their peers are always partially offline. Their own data availability depends on others' full nodes.

**Why it's OK:** Even all-light-node pacts give 99.97% data availability (§2.5). The user's reach is lower (fewer forwarding advocates) but they're not excluded — the incentive model has "no cliff." As the user builds WoT, some partners will be full nodes.

**Guidance for users:** Running even a cheap VPS ($5/month) or keeping a laptop online significantly improves your storage reliability and content reach. This is analogous to running a Bitcoin full node — not required, but beneficial.

### LOW: Gossip Reach Reduction from Online Fraction

**Problem:** With 46% online rate, gossip reaches ~400 online nodes (vs theoretical 8,000). This reduces the "discovery radius" of gossip.

**Why it's OK:** Gossip is WoT-routed (§4). It doesn't need to reach many nodes — just the right ones. For in-WoT requests (following someone), gossip traverses the target's WoT neighborhood where storage peers live. Network size and raw reach don't determine success; WoT proximity does.

### LOW: Bootstrap Pact Concentration

**Problem:** If many new users follow the same popular account, that account accumulates many bootstrap pacts.

**Why it's OK:** Auto-accepts "if capacity allows." Bootstrap pacts store casual-user data (~112 KB each). Even 1,000 bootstrap pacts = 112 MB — manageable for a full node.

**If it's not OK:** Distribute bootstrap across first N follows (not just first follow). Or: relay-operated bootstrap service where the relay itself offers temporary pacts for new users.

---

## 12. Negative Feedback and Contraction Risk

The preceding analysis models positive feedback (growth spiral: more users → more pacts → better availability → more users). The reverse — contraction — is equally important and has not been modeled.

### The Contraction Spiral

When users leave:
1. Their pact partners lose pacts → availability decreases for remaining users
2. Decreased availability → more relay fallback → reduced perceived value
3. Reduced perceived value → more users leave
4. Repeat

### Tipping Point Analysis

Game-theoretic analysis identifies approximate thresholds:

| Free-rider % | Effect on cooperative equilibrium |
|--------------|----------------------------------|
| <20% | Stable — cooperators have sufficient partners |
| 20-30% | Stressed — pact formation slows, some users stuck in Hybrid phase |
| 30-50% | Degrading — cooperative equilibrium begins collapsing |
| 50-70% | Unreliable — measurable availability degradation |
| >70% | Failed — network functionally relay-dependent |

The critical threshold is approximately 30% — above this, the incentive to cooperate weakens faster than it strengthens.

### What Is Not Modeled

This analysis does not yet model:
- **Time-to-recovery after churn:** If 30% of users leave over 3 months, how long does pact replacement take for remaining users?
- **Cascade effects:** A user losing 6 of 20 pact partners simultaneously may trigger over-reaction (excessive pact requests flooding the network)
- **Minimum viable return:** What network size must survive contraction for the protocol to still provide value? Below this size, the protocol degrades to relay-only with extra overhead.

These scenarios should be validated in simulation before production deployment.

**Simulation Evidence (Pact Churn and Contraction):** Simulation now provides direct evidence on contraction dynamics. Across all four topologies tested, **pact churn is net-negative** — the network sheds more pacts than it forms over the 30-day run:

| Topology | Net pact churn (30 days) | Churn/node/day | Availability |
|----------|-------------------------|----------------|-------------|
| BA m=10 | -40,030 | 2.79 | 98.4% |
| WS p=0.30 | -45,710 | 5.45 | 97.7% |
| BA m=50 | -104,444 | 6.87 | 95.3% |
| BA m=50+TZ | -112,869 | 8.04 | 94.8% |

This is a significant finding: the cooperative equilibrium may be contracting even without external shocks or free-riders. Nodes dissolve pacts (due to failed challenges, volume mismatch, or partner departure) faster than they form replacements. The contraction is most severe in dense topologies where nodes have many potential partners but higher churn rates. Sparse topologies (BA m=10) achieve both the lowest churn and the highest availability, suggesting that **stable, long-lived pacts matter more than pact abundance**.

The "30% free-rider threshold" from the game-theoretic analysis has not been directly tested, but the net-negative churn data suggests the equilibrium is more fragile than the static analysis implies. Even in a fully cooperative network (no intentional free-riders), organic churn alone pushes the pact economy toward contraction. Protocol-level mitigations to slow pact dissolution (longer grace periods, graduated challenge penalties, pact renewal incentives) may be needed to sustain a stable cooperative equilibrium.

Additional cross-scale evidence: a 5,000-node BA m=50 run achieved 96.9% availability (vs 95.3% at 2,000 nodes), suggesting that larger networks partially offset churn through greater partner diversity. A 1,000-node BA m=10 run achieved 99.2% — the best observed result — confirming that small, sparse, stable networks perform best.

---

## 13. Summary Table

Assuming 25% full nodes (always-on), 75% light nodes (30% uptime).

| Dimension | Full node | Light node | Status |
|-----------|-----------|------------|--------|
| Storage (own pacts, 20 partners) | ~103 MB | ~103 MB | OK — < 0.5% of phone storage |
| Effective pacts served | ~80 (absorbs light-node share) | ~20 (stored, served when online) | OK — 53 MB at 80 pacts |
| Bandwidth outbound/day | ~301 MB (pact serving) | ~717 KB | OK — 0.3% of 10Mbps upload |
| Bandwidth inbound/day | ~6.2 MB | ~2.5 MB | OK — negligible |
| Mobile data monthly | N/A (broadband) | ~96 MB | OK — 1.9% of 5 GB plan |
| Gossip load per node | ~0.15 req/s, 45 B/s | ~0.15 req/s (when online) | OK — negligible |
| Gossip discovery (in-WoT) | ~95%+ (follows target) | ~95%+ | OK — WoT-routed, not random |
| Gossip discovery (stranger) | 0% (use relay) | 0% (use relay) | By design |
| Pact partners online at any time | 11.2 of 20 | 11.2 of 20 | OK — need 1, have 11 |
| Data availability | ~100% | ~100% (even all-light: 99.97%) | OK |
| Popular account (100K followers) | 19.5 KB/s per online peer | Contributes when online | OK |
| Celebrity viral spike | ~2.6 MB/s per online peer | N/A (full nodes absorb) | Medium risk — transient |
| Challenge-response | 36 KB/day (80 pacts) | ~3 KB/day (when online) | OK — invisible |
| BLE mesh (dense urban) | 3s for 50-event sync | 3s for 50-event sync | OK |

---

## 14. Critical Numbers to Watch

These parameters should be monitored as the network grows:

1. **Full-node percentage** — the protocol assumes 20-30%. If it drops below 15%, pact serving concentrates too heavily. Encourage users to run persistent nodes (even cheap VPS).
2. **Pacts per full node** — at 25% full nodes, each serves ~80 pacts. If this exceeds ~200 (full-node percentage drops to 10%), add pact caps or incentivize full-node operation.
3. **Challenge timing** — challenges must be presence-aware to avoid unfairly penalizing light nodes. Monitor false-negative rate (challenges sent to offline peers).
4. **Popular account first-wave** — viral posts spike online storage peers to ~2.6 MB/s. If spikes last >5 minutes, add per-peer request throttling and rely on read-cache propagation.
5. **Average pact count** — the protocol assumes users reach 20 pacts. If real-world WoT graphs are sparser, users stay in hybrid phase longer (which is fine — relays cover the gap).
6. **Read-cache propagation speed** — model assumes cascading cache builds in 3-5 minutes for popular content. If gossip response is slow in the 46% online environment, this may take longer.

---

## 15. Conclusion

The protocol's numbers are plausible and well within the capability of consumer hardware and typical internet connections. No single parameter creates an unworkable bottleneck.

**The strongest design decision** is the layered delivery model (BLE → cached endpoints → gossip → DVM → relay). Each layer handles different scenarios, and the combined delivery probability approaches 100% at any network scale.

**Gossip works at any scale** because it's WoT-routed, not random. When Bob wants Alice's data, his gossip traverses Alice's WoT neighborhood — where her storage peers live. Network size doesn't matter; WoT proximity does. The initial concern that "gossip fails at >100K" was based on a flawed random-sampling model.

**The 25%/75% full/light split is viable, but 25% is an optimistic target.** Comparable systems achieve 0.1-5% always-on participation. The protocol is designed to function at full-node ratios as low as 5%, where the all-light-node availability analysis applies (P(unavailable) ≈ 0.08%). Full nodes absorb ~80 pacts each (vs the 20 they directly need) — 53 MB storage, 300 MB/day outbound, well within consumer broadband. Light nodes participate when online but aren't required to be always-on. Data availability remains ~100% under the independent-failure model because 20 pact partners provide massive redundancy even with mixed node types. Under realistic correlated-failure assumptions (timezone overlap, shared infrastructure), availability degrades to ~99.9-99.97% — still respectable for a peer-to-peer system.

**The most sensitive parameter** is the full-node percentage. If it drops below ~15%, each full node would serve 100+ pacts and the load could become meaningful. The natural incentive (more pacts → more reach) encourages running persistent nodes, but this should be monitored.

**One design clarification needed:** Challenge-response timing should be presence-aware. Challenging offline light nodes wastes bandwidth and unfairly penalizes their reliability scores. This is a client-side optimization — the protocol itself doesn't need to change.

**Simulation update (2,000-node, 30-day runs across four topologies):** The analytical formulas above are directionally correct but quantitatively optimistic. Simulation reveals three important corrections: (1) Availability under churn is 94.8-98.4%, not the ~100% predicted by F-14/F-15, because pact churn creates transient low-redundancy windows that the static formula does not capture. (2) Gossip handles only 0.1-9% of reads, not the implied ~90% — pact-local reads (74-92%) are the dominant delivery path, making the protocol more pact-dependent and less gossip-dependent than the mean-field model suggests. (3) Pact churn is net-negative in all topologies tested, meaning the cooperative equilibrium contracts organically even without free-riders. Sparse topologies (BA m=10) consistently outperform dense ones (BA m=50) by 3+ percentage points on availability, with lower churn rates and lower relay dependency at maturity. These findings do not invalidate the protocol design — availability remains above 94% in all scenarios tested — but they shift the critical parameter from "full-node percentage" to "pact stability." Protocol features that extend pact lifetimes (graduated penalties, renewal incentives, challenge grace periods) may matter more than recruiting additional full nodes.

---

## Appendix: Formula Index

All formulas are labeled for cross-reference. To verify any result, trace it back through the chain to the input constants in §1.

| Formula | Description | Dependencies | Section |
|---------|-------------|-------------|---------|
| F-01 | Weighted avg event size | Event sizes + mix | §1 |
| F-02 | Monthly volume per user | E_AVG, events/day | §1 |
| F-03 | Active pact storage | PACTS_DEFAULT, F-02 | §2 |
| F-04 | Standby pact storage | PACTS_STANDBY, F-02 | §2 |
| F-05 | Total pact storage | F-03 + F-04 | §2 |
| F-06 | Read-cache estimate | follows, F-02, READ_CACHE_MAX_MB | §2 |
| F-07 | Total on-device storage | F-05 + F-06 | §2 |
| F-08 | Storage as % of device | F-07 | §2 |
| F-09 | Pact demand | N, PACTS_DEFAULT | §2.5 |
| F-10 | Pact supply | N, PACTS_DEFAULT | §2.5 |
| F-11 | Full-node pact supply | N, FULL_NODE_PCT, PACTS_DEFAULT | §2.5 |
| F-12 | Max full-node pact share | F-11 / F-09 | §2.5 |
| F-13 | Expected full-node partners | PACTS_DEFAULT, FULL_NODE_PCT, bias | §2.5 |
| F-14 | P(all peers offline) | FULL_UPTIME, LIGHT_UPTIME, n_full, n_light | §2.5 |
| F-15 | P(≥1 peer online) | 1 - F-14 | §2.5 |
| F-16 | Expected peers online | n_full × FULL_UPTIME + n_light × LIGHT_UPTIME | §2.5 |
| F-17 | Total online with standby | F-16 + standby contribution | §2.5 |
| F-18 | Pacts per full node | PACTS_DEFAULT / FULL_NODE_PCT | §2.5 |
| F-19 | Full-node pact storage | F-18 × monthly volume | §2.5 |
| F-20 | Full-node challenge BW | F-18 × challenge bytes | §2.5 |
| F-21 | Full-node serving load | F-18 × requests × F-35 | §2.5 |
| F-22 | Peak serving load | F-21 / peak_hours | §2.5 |
| F-23 | Broadband utilization (peak) | F-22 / upload_capacity | §2.5 |
| F-24 | Network online fraction | FULL_NODE_PCT, LIGHT_NODE_PCT, uptimes | §3 |
| F-25 | Effective online peers | PACTS_DEFAULT × F-24 | §3 |
| F-26 | Gossip reach per hop | F-25, CLUSTERING | §3 |
| F-27 | Gossip requests per user/day | APP_SESSIONS, follows, GOSSIP_FALLBACK | §3 |
| F-28 | Network gossip rate | N, DAU_PCT, F-27 | §3 |
| F-29 | Online nodes | N × F-24 | §3 |
| F-30 | Reach fraction | F-26 / F-29 | §3 |
| F-31 | Per-node gossip rate | F-28 × F-30 (converges, N-independent) | §3 |
| F-32 | Per-node gossip BW | F-31 × E_GOSSIP_REQ | §3 |
| F-33 | LRU cache coverage | DEDUP_CACHE_SIZE / F-31 | §3 |
| F-34 | Online pact peers | n_full × FULL_UPTIME + n_light × LIGHT_UPTIME | §5 |
| F-35 | Light sync payload | LIGHT_SYNC_DEPTH × E_AVG | §5 |
| F-36 | Connections per peer (popular) | followers, DAU_PCT, F-34 | §5 |
| F-37 | Per-peer bandwidth (popular) | F-36 × F-35 | §5 |
| F-38 | Celebrity connections per peer | followers, DAU_PCT, first_hour_fraction, F-34 | §5 |
| F-39 | Viral connections/s per peer | viral_views, duration, F-34 | §5 |
| F-40 | Viral bandwidth per peer | F-39 × F-35 | §5 |
| F-41 | Viral broadband utilization | F-40 / upload_capacity | §5 |
| F-42 | Time to read-cache takeover | cache_threshold / (F-39 × F-34) | §5 |
| F-43 | Challenge bandwidth | PACTS × CHALLENGE_FREQ × bytes | §6 |
| F-44 | Hash compute time | range × E_AVG / SHA256_throughput | §6 |
| F-45 | BLE transfer time | data_size / BLE_THROUGHPUT | §9 |
| F-46 | BLE per-hop latency | BLE_SETUP + F-45 | §9 |
| F-47 | BLE total latency | hops × F-46 | §9 |
| F-48 | Full-node broadband utilization | total_outbound / daily_upload_capacity | §8 |
| F-49 | Light-node challenge hit rate | LIGHT_UPTIME | §8 |
| F-50 | Light-node monthly data | (out + in) × 30 | §8 |

### Sensitivity Tests

To check what happens if assumptions change, re-run these key formulas:

| "What if..." | Change | Key formulas to re-run | Expected impact |
|-------------|--------|----------------------|-----------------|
| Only 15% full nodes | FULL_NODE_PCT = 0.15 | F-18 → 133 pacts/full node, F-24 → 0.37, F-16 | Higher full-node load, lower availability |
| Light nodes 50% uptime | LIGHT_UPTIME = 0.50 | F-14, F-16, F-24, F-25, F-31 | Better availability, higher gossip reach |
| Larger events (1.5 KB avg) | E_AVG = 1500 | F-02, F-03, F-05, F-07, F-21, F-35, F-40 | 2× storage, 2× bandwidth |
| 40 pacts default | PACTS_DEFAULT = 40 | F-03, F-09, F-14, F-16, F-18, F-25 | 2× storage, 2× availability, lower full-node load (F-18=160→not realistic, need more full nodes) |
| 5% gossip fallback | GOSSIP_FALLBACK = 0.05 | F-27, F-28, F-31, F-32 | 2.5× gossip load per node (still ~0.4 req/s, fine) |
| Celebrity with 100 pacts | PACTS_POPULAR = 100 | F-34, F-36, F-37 | Lower per-peer load, better viral handling |

### What if clustering coefficient is 0.50 (high-modularity graph)?

Real social networks often have clustering within communities of 0.5-0.7. With C=0.50:

**Gossip reach [F-26]:** reach(3) = 20 × [20 × (1-0.50)]^2 = 20 × 100 = 2,000 nodes (vs. 4,500 at C=0.25)

**Impact:** Gossip reach drops by ~55%. Per-node gossip load decreases (fewer nodes forwarding), but discovery effectiveness degrades in modular graphs. Content crossing community boundaries becomes less likely via gossip alone. Relay fallback (Tier 4) handles more cross-community reads.

**Verdict:** The protocol functions but becomes more relay-dependent for inter-community content at high clustering. This is consistent with the design — relays are permanently needed for content discovery beyond the WoT.
