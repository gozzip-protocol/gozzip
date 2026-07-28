# Feed Model Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement the feed model (Inner Circle / Orbit / Horizon tiers with interaction-based referral and tiered caching) in the Rust simulator.

**Architecture:** Rename the existing WotTier enum and config fields to match the feed model terminology (InnerCircle, Orbit, Horizon). Add interaction tracking to the orchestrator: when events of kind Reaction/Repost are generated, record the interaction toward a target author. Use accumulated interaction data to dynamically expand Orbit membership via the referral mechanism (3+ IC contacts interacting with an author). Add tiered cache TTLs to evict non-pact content based on feed tier.

**Tech Stack:** Rust, tokio, serde, lru crate, TOML config

---

### Task 1: Rename WotTier enum variants

**Files:**
- Modify: `simulator/src/types.rs:52-57`

**Step 1: Write the failing test**

Add a test in `simulator/src/types.rs` that asserts the new variant names exist:

```rust
#[test]
fn test_feed_tier_variants() {
    let ic = WotTier::InnerCircle;
    let orbit = WotTier::Orbit;
    let horizon = WotTier::Horizon;
    assert_ne!(format!("{:?}", ic), format!("{:?}", orbit));
    assert_ne!(format!("{:?}", orbit), format!("{:?}", horizon));
}
```

**Step 2: Run test to verify it fails**

Run: `cd simulator && cargo test test_feed_tier_variants`
Expected: FAIL — `InnerCircle` variant doesn't exist

**Step 3: Rename the enum variants**

In `simulator/src/types.rs`, change:
```rust
pub enum WotTier {
    InnerCircle,   // Mutual follow — pact-stored, continuous sync
    Orbit,         // High-interaction + socially-endorsed authors
    Horizon,       // 2-hop graph + relay discoveries
}
```

Then fix all references across the codebase. Every file that matches `DirectWot` → `InnerCircle`, `OneHop` → `Orbit`, `TwoHop` → `Horizon`:
- `simulator/src/sim/orchestrator.rs` (~3 references in tier selection logic, lines 460-494)
- `simulator/src/node/mod.rs` (in ReadRequest handling and MetricEvent emission)
- `simulator/src/sim/metrics.rs` (in ReadResultRecord usage and JSONL emission)
- `simulator/src/scenarios/validate.rs` (in `print_read_tier_by_wot`, lines 843-853 — also rename labels: `"direct-wot"` → `"inner-circle"`, `"1-hop"` → `"orbit"`, `"2-hop"` → `"horizon"`)
- `simulator/src/output/json.rs` (test data using `WotTier::OneHop`)

**Step 4: Run all tests**

Run: `cd simulator && cargo test`
Expected: All tests pass (pure rename, no behavior change)

**Step 5: Commit**

```
git add simulator/src/
git commit -m "refactor: rename WotTier variants to feed model terminology"
```

---

### Task 2: Rename config fields and update default weights

**Files:**
- Modify: `simulator/src/config.rs:196-217`
- Modify: `simulator/config/default.toml`
- Modify: `simulator/config/realistic-5k.toml`

**Step 1: Write the failing test**

In `simulator/src/config.rs` tests:

```rust
#[test]
fn test_feed_weight_defaults() {
    let cfg = SimConfig::default();
    assert_relative_eq!(cfg.retrieval.feed_weight_inner_circle, 0.60);
    assert_relative_eq!(cfg.retrieval.feed_weight_orbit, 0.25);
    assert_relative_eq!(cfg.retrieval.feed_weight_horizon, 0.15);
}
```

**Step 2: Run test to verify it fails**

Run: `cd simulator && cargo test test_feed_weight_defaults`
Expected: FAIL — field doesn't exist

**Step 3: Rename fields and update defaults**

In `simulator/src/config.rs` `RetrievalConfig`:

```rust
pub struct RetrievalConfig {
    pub reads_per_day: u32,
    pub read_timeout_secs: f64,
    pub relay_success_rate: f64,
    #[serde(default = "default_relay_stagger")]
    pub relay_stagger_secs: f64,
    #[serde(default = "default_feed_weight_inner_circle", alias = "wot_weight_direct")]
    pub feed_weight_inner_circle: f64,
    #[serde(default = "default_feed_weight_orbit", alias = "wot_weight_one_hop")]
    pub feed_weight_orbit: f64,
    #[serde(default = "default_feed_weight_horizon", alias = "wot_weight_two_hop")]
    pub feed_weight_horizon: f64,
}
```

Update default functions:
```rust
fn default_feed_weight_inner_circle() -> f64 { 0.60 }
fn default_feed_weight_orbit() -> f64 { 0.25 }
fn default_feed_weight_horizon() -> f64 { 0.15 }
```

Remove old `default_wot_weight_*` functions.

Update `SimConfig::default()` in the retrieval block:
```rust
retrieval: RetrievalConfig {
    // ...
    feed_weight_inner_circle: 0.60,
    feed_weight_orbit: 0.25,
    feed_weight_horizon: 0.15,
},
```

Update all references in `orchestrator.rs` (lines 446-448):
```rust
let raw = [
    (cfg.feed_weight_inner_circle, n_direct),
    (cfg.feed_weight_orbit, n_one_hop),
    (cfg.feed_weight_horizon, n_two_hop),
];
```

Update existing test `test_default_config_values` to use new field names.

**Note:** The `alias` attributes on the serde fields ensure old TOML configs that use `wot_weight_direct` etc. still parse correctly.

**Step 4: Run all tests**

Run: `cd simulator && cargo test`
Expected: All pass

**Step 5: Commit**

```
git add simulator/
git commit -m "refactor: rename feed weight config fields, update defaults to 60/25/15"
```

---

### Task 3: Add interaction tracking infrastructure

**Files:**
- Modify: `simulator/src/types.rs` — add `interaction_target` field to Event
- Modify: `simulator/src/config.rs` — add interaction weight config
- Modify: `simulator/src/sim/orchestrator.rs` — add InteractionTracker struct

**Step 1: Write the failing test**

In `simulator/src/types.rs` tests:

```rust
#[test]
fn test_event_interaction_target() {
    let event = Event {
        id: 1,
        author: 10,
        kind: EventKind::Reaction,
        size_bytes: 500,
        seq: 0,
        prev_hash: 0,
        created_at: 1.0,
        nostr_json: None,
        interaction_target: Some(42),
    };
    assert_eq!(event.interaction_target, Some(42));
}
```

**Step 2: Run test to verify it fails**

Run: `cd simulator && cargo test test_event_interaction_target`
Expected: FAIL — field doesn't exist

**Step 3: Add the field and config**

In `simulator/src/types.rs` `Event` struct, add:
```rust
pub struct Event {
    // ... existing fields ...
    /// For Reaction/Repost events, the author being interacted with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction_target: Option<NodeId>,
}
```

Fix all `Event { ... }` construction sites to include `interaction_target: None` (orchestrator.rs event generation, node/state.rs test helpers, etc.).

In `simulator/src/config.rs`, add to `RetrievalConfig`:
```rust
/// Interaction score weights for feed ranking and referral.
#[serde(default = "default_interaction_weight_reply")]
pub interaction_weight_reply: f64,
#[serde(default = "default_interaction_weight_repost")]
pub interaction_weight_repost: f64,
#[serde(default = "default_interaction_weight_reaction")]
pub interaction_weight_reaction: f64,
/// Decay half-life in days for interaction scores.
#[serde(default = "default_interaction_decay_days")]
pub interaction_decay_days: f64,
/// Minimum number of IC contacts interacting to trigger referral.
#[serde(default = "default_referral_min_contacts")]
pub referral_min_contacts: u32,
/// Minimum interaction score for a contact to count toward referral.
#[serde(default = "default_referral_min_score")]
pub referral_min_score: f64,
```

Default functions:
```rust
fn default_interaction_weight_reply() -> f64 { 3.0 }
fn default_interaction_weight_repost() -> f64 { 2.0 }
fn default_interaction_weight_reaction() -> f64 { 1.0 }
fn default_interaction_decay_days() -> f64 { 30.0 }
fn default_referral_min_contacts() -> u32 { 3 }
fn default_referral_min_score() -> f64 { 5.0 }
```

Update `SimConfig::default()` with these new fields.

**Step 4: Run all tests**

Run: `cd simulator && cargo test`
Expected: All pass

**Step 5: Commit**

```
git add simulator/src/
git commit -m "feat: add interaction_target field to Event and interaction config"
```

---

### Task 4: Implement interaction scoring in orchestrator

**Files:**
- Modify: `simulator/src/sim/orchestrator.rs` — track interactions during event generation

**Step 1: Write the failing test**

Create a unit test module in orchestrator or a separate test file. Since orchestrator tests are integration-level, we'll test via the InteractionTracker directly. Add to the bottom of `orchestrator.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interaction_tracker_basic() {
        let mut tracker = InteractionTracker::new(30.0);
        // Node 0 reacts to author 5 at time 100.0 with weight 1.0
        tracker.record(0, 5, 1.0, 100.0);
        tracker.record(0, 5, 1.0, 100.0);
        let score = tracker.score(0, 5, 100.0);
        assert!(score > 1.9 && score < 2.1); // ~2.0, no decay
    }

    #[test]
    fn test_interaction_tracker_decay() {
        let mut tracker = InteractionTracker::new(30.0);
        tracker.record(0, 5, 1.0, 0.0); // at day 0
        let score_at_21d = tracker.score(0, 5, 21.0 * 86400.0);
        // exp(-21/30) ≈ 0.497
        assert!(score_at_21d > 0.4 && score_at_21d < 0.6);
    }

    #[test]
    fn test_interaction_tracker_referrals() {
        let mut tracker = InteractionTracker::new(30.0);
        let ic_contacts = vec![1, 2, 3, 4];
        let time = 100.0;
        // 3 of 4 IC contacts interact with author 99
        tracker.record(1, 99, 3.0, time); // reply
        tracker.record(2, 99, 2.0, time); // repost
        tracker.record(3, 99, 1.0, time); // reaction
        let referrals = tracker.compute_referrals(&ic_contacts, 3, 0.5, time);
        assert!(referrals.contains(&99));
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cd simulator && cargo test test_interaction_tracker_basic`
Expected: FAIL — `InteractionTracker` doesn't exist

**Step 3: Implement InteractionTracker**

Add to `simulator/src/sim/orchestrator.rs` (before the `Orchestrator` struct):

```rust
/// Tracks interaction scores between nodes for feed referral mechanism.
/// Key: (interactor, target_author), Value: Vec<(weight, timestamp)>
struct InteractionTracker {
    /// Raw interaction events: (interactor, target) → [(weight, time)]
    interactions: HashMap<(NodeId, NodeId), Vec<(f64, SimTime)>>,
    /// Decay half-life in seconds (converted from days at construction).
    decay_secs: f64,
}

impl InteractionTracker {
    fn new(decay_days: f64) -> Self {
        Self {
            interactions: HashMap::new(),
            decay_secs: decay_days * 86400.0,
        }
    }

    fn record(&mut self, interactor: NodeId, target: NodeId, weight: f64, time: SimTime) {
        self.interactions
            .entry((interactor, target))
            .or_default()
            .push((weight, time));
    }

    fn score(&self, interactor: NodeId, target: NodeId, now: SimTime) -> f64 {
        self.interactions
            .get(&(interactor, target))
            .map(|entries| {
                entries.iter().map(|(w, t)| {
                    let age_days = (now - t) / 86400.0;
                    w * (-age_days / (self.decay_secs / 86400.0)).exp()
                }).sum()
            })
            .unwrap_or(0.0)
    }

    /// Find authors that should enter a reader's Orbit via referral.
    /// Returns authors where >= min_contacts IC contacts have scores >= min_score.
    fn compute_referrals(
        &self,
        ic_contacts: &[NodeId],
        min_contacts: u32,
        min_score: f64,
        now: SimTime,
    ) -> HashSet<NodeId> {
        // Collect all authors that any IC contact interacts with
        let mut author_endorsers: HashMap<NodeId, u32> = HashMap::new();
        for &contact in ic_contacts {
            // Find all targets this contact has interacted with
            for (&(interactor, target), _) in &self.interactions {
                if interactor == contact {
                    let s = self.score(interactor, target, now);
                    if s >= min_score {
                        *author_endorsers.entry(target).or_insert(0) += 1;
                    }
                }
            }
        }
        author_endorsers
            .into_iter()
            .filter(|(_, count)| *count >= min_contacts)
            .map(|(author, _)| author)
            .collect()
    }
}
```

Then integrate into the event generation loop. In the orchestrator's `run()` method, after generating an event:

```rust
// After choosing event kind, if Reaction or Repost, pick a target author
let interaction_target = if kind == EventKind::Reaction || kind == EventKind::Repost {
    // Pick a random follow of the author as interaction target
    if let Some(follows) = self.graph.follows.get(&author) {
        if !follows.is_empty() {
            let pool: Vec<NodeId> = follows.iter().copied().collect();
            let target = pool[self.rng.gen_range(0..pool.len())];
            let weight = match kind {
                EventKind::Repost => self.config.retrieval.interaction_weight_repost,
                EventKind::Reaction => self.config.retrieval.interaction_weight_reaction,
                _ => 0.0,
            };
            self.interaction_tracker.record(author, target, weight, time);
            Some(target)
        } else {
            None
        }
    } else {
        None
    }
} else {
    None
};
```

Set `interaction_target` on the Event being constructed.

Add `interaction_tracker: InteractionTracker` to the `Orchestrator` struct and initialize it in `new()` and `with_graph()`:
```rust
interaction_tracker: InteractionTracker::new(config.retrieval.interaction_decay_days),
```

**Step 4: Run all tests**

Run: `cd simulator && cargo test`
Expected: All pass

**Step 5: Commit**

```
git add simulator/src/
git commit -m "feat: implement interaction tracking in orchestrator"
```

---

### Task 5: Implement referral-based Orbit expansion

**Files:**
- Modify: `simulator/src/sim/orchestrator.rs` — expand Orbit during read selection using referrals

**Step 1: Write the failing test**

Add to orchestrator tests:

```rust
#[test]
fn test_orbit_expansion_includes_referrals() {
    let mut tracker = InteractionTracker::new(30.0);
    let time = 1000.0;
    // IC contacts 1, 2, 3 all interact with author 99
    for &contact in &[1u32, 2, 3] {
        tracker.record(contact, 99, 5.0, time);
    }
    let ic_contacts = vec![1, 2, 3];
    let referrals = tracker.compute_referrals(&ic_contacts, 3, 0.5, time);
    assert!(referrals.contains(&99));

    // Only 2 contacts interact with author 88 — below threshold
    tracker.record(1, 88, 5.0, time);
    tracker.record(2, 88, 5.0, time);
    let referrals2 = tracker.compute_referrals(&ic_contacts, 3, 0.5, time);
    assert!(!referrals2.contains(&88));
}
```

**Step 2: Run test to verify it fails**

Run: `cd simulator && cargo test test_orbit_expansion`
Expected: PASS (already implemented in Task 4), or FAIL if we haven't wired it into read selection yet.

**Step 3: Wire referrals into read selection**

In the orchestrator's read generation loop (around line 427-516), after getting the one_hop_set for the reader, expand it with referrals:

```rust
// Compute referrals for this reader's Orbit expansion
let ic_contacts: Vec<NodeId> = direct
    .map(|s| s.iter().copied().collect())
    .unwrap_or_default();
let referrals = self.interaction_tracker.compute_referrals(
    &ic_contacts,
    self.config.retrieval.referral_min_contacts,
    self.config.retrieval.referral_min_score,
    time,
);

// Orbit = one_hop ∪ referrals (excluding self, IC members, and existing one_hop)
let mut orbit_set: HashSet<NodeId> = one_hop_set
    .cloned()
    .unwrap_or_default();
for author in &referrals {
    if *author != reader
        && !direct.map_or(false, |d| d.contains(author))
    {
        orbit_set.insert(*author);
    }
}
let n_orbit = orbit_set.len();
```

Then use `n_orbit` and `orbit_set` instead of `n_one_hop` and `one_hop_set` in the weight calculation and author selection.

**Step 4: Run all tests**

Run: `cd simulator && cargo test`
Expected: All pass

**Step 5: Commit**

```
git add simulator/src/sim/orchestrator.rs
git commit -m "feat: wire referral-based Orbit expansion into read selection"
```

---

### Task 6: Add tiered cache TTLs

**Files:**
- Modify: `simulator/src/config.rs` — add cache TTL config
- Modify: `simulator/src/types.rs` — add FeedTier to cached entries (or tag in Message)
- Modify: `simulator/src/node/state.rs` — implement TTL-based eviction
- Modify: `simulator/src/node/mod.rs` — pass feed tier through to cache

**Step 1: Write the failing test**

In `simulator/src/config.rs` tests:

```rust
#[test]
fn test_cache_ttl_defaults() {
    let cfg = SimConfig::default();
    assert_relative_eq!(cfg.retrieval.orbit_cache_ttl_days, 14.0);
    assert_relative_eq!(cfg.retrieval.horizon_cache_ttl_days, 3.0);
    assert_relative_eq!(cfg.retrieval.relay_cache_ttl_days, 1.0);
}
```

**Step 2: Run test to verify it fails**

Run: `cd simulator && cargo test test_cache_ttl_defaults`
Expected: FAIL — field doesn't exist

**Step 3: Add config fields**

In `simulator/src/config.rs` `RetrievalConfig`, add:

```rust
#[serde(default = "default_orbit_cache_ttl_days")]
pub orbit_cache_ttl_days: f64,
#[serde(default = "default_horizon_cache_ttl_days")]
pub horizon_cache_ttl_days: f64,
#[serde(default = "default_relay_cache_ttl_days")]
pub relay_cache_ttl_days: f64,
```

Default functions:
```rust
fn default_orbit_cache_ttl_days() -> f64 { 14.0 }
fn default_horizon_cache_ttl_days() -> f64 { 3.0 }
fn default_relay_cache_ttl_days() -> f64 { 1.0 }
```

Update `SimConfig::default()`.

**Step 4: Implement TTL-based eviction**

In `simulator/src/node/state.rs`, modify `cache_events` to store the cache timestamp:

Change `read_cache` type from `LruCache<NodeId, Vec<Event>>` to `LruCache<NodeId, (Vec<Event>, SimTime)>` where the SimTime is the cache insertion time.

Add a method to evict expired cache entries:

```rust
/// Evict cached entries older than their tier-based TTL.
pub fn evict_expired_cache(&mut self, current_time: SimTime, ttl_secs: f64) {
    // LruCache doesn't support iteration + removal easily,
    // so collect expired keys first
    let mut expired = Vec::new();
    for (author, (_, cached_at)) in self.read_cache.iter() {
        if current_time - cached_at > ttl_secs {
            expired.push(*author);
        }
    }
    for author in expired {
        self.read_cache.pop(&author);
    }
}
```

Update all `read_cache.put()` calls to include the current time.
Update all `read_cache.get()` calls to extract just the events.

In `node/mod.rs`, call `evict_expired_cache` on each `Tick` message, using the Horizon TTL (shortest) as the minimum eviction sweep. In practice, use a single TTL for the read_cache since we don't track which tier each entry came from — use `horizon_cache_ttl_days` as the conservative default (3 days). Content that's accessed frequently stays via LRU; stale content ages out via TTL.

**Step 5: Run all tests**

Run: `cd simulator && cargo test`
Expected: All pass

**Step 6: Commit**

```
git add simulator/src/
git commit -m "feat: add tiered cache TTLs with time-based eviction"
```

---

### Task 7: Update reporting and validate scenario

**Files:**
- Modify: `simulator/src/scenarios/validate.rs` — update tier labels in print output
- Modify: `simulator/src/output/json.rs` — update test data

**Step 1: Update validate.rs tier labels**

The tier labels in `print_read_tier_by_wot` should already be updated from Task 1. Verify the label strings are:
- `"inner-circle"` (was `"direct-wot"`)
- `"orbit"` (was `"1-hop"`)
- `"horizon"` (was `"2-hop"`)

Also rename the function from `print_read_tier_by_wot` to `print_read_tier_by_feed_tier`.

**Step 2: Run the full validation**

Run: `cd simulator && cargo run --release -- --nodes 100 validate`
Expected: Simulation completes, tier labels show correctly in output

**Step 3: Run all tests one final time**

Run: `cd simulator && cargo test`
Expected: All pass

**Step 4: Commit**

```
git add simulator/src/
git commit -m "feat: complete feed model implementation with updated reporting"
```

---

### Task 8: Update TOML configs and run full validation

**Files:**
- Modify: `simulator/config/default.toml`
- Modify: `simulator/config/realistic-5k.toml`

**Step 1: Add new config keys to TOML files**

In both TOML files, add under `[retrieval]`:

```toml
[retrieval]
reads_per_day = 50
read_timeout_secs = 30.0
relay_success_rate = 0.80
relay_stagger_secs = 2.0
feed_weight_inner_circle = 0.60
feed_weight_orbit = 0.25
feed_weight_horizon = 0.15
orbit_cache_ttl_days = 14.0
horizon_cache_ttl_days = 3.0
relay_cache_ttl_days = 1.0
```

**Step 2: Run validation with both configs**

Run: `cd simulator && cargo run --release -- --nodes 100 validate`
Run: `cd simulator && cargo run --release -- --config config/realistic-5k.toml validate` (if VPS, otherwise skip)

**Step 3: Commit**

```
git add simulator/config/ simulator/src/
git commit -m "docs: update TOML configs with feed model settings"
```
