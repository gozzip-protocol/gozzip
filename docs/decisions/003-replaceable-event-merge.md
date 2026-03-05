# ADR 003: Replaceable Event Merge (prev tag + Automatic Merge)

**Date:** 2026-02-28
**Status:** Accepted

## Context

Replaceable events (kind 0 profile, kind 3 follow list) are the only event types where multi-device conflict is possible. Two devices can independently update the same replaceable event while offline, creating a fork.

Pure last-write-wins (Nostr default) silently drops one side's changes. If device A follows 5 new people and device B follows 3 different people, the later-timestamped event wins and the earlier side's follows are lost. This is unacceptable for a Twitter-like experience.

We need fork detection and automatic merge.

## Decision

Add a `prev` tag to kind 0 and kind 3 events pointing to the event ID being replaced. This enables fork detection. When a fork is detected, apply automatic merge rules per event type.

### Fork detection

Every kind 0 and kind 3 event includes:

```json
["prev", "<event_id_of_replaced_event>"]
```

A fork exists when two events reference the same `prev` value — they were both replacing the same ancestor.

### Follow list merge (kind 3)

Set-based diff against the common ancestor:

```
merged = ancestor + added_by_A + added_by_B - removed_by_A - removed_by_B
```

- `added_by_X` = tags in X's version but not in ancestor
- `removed_by_X` = tags in ancestor but not in X's version

**Conflict rule:** If one side adds (follows) and the other removes (unfollows) the same pubkey, **follow wins**. Rationale: it is less harmful to keep a follow than to lose one. The user can always unfollow again; a silently dropped follow may never be noticed.

> **Updated by [ADR 008](008-protocol-hardening.md):** This conflict rule has been superseded. The current rule is **most recent action wins** — compare timestamps of the follow/unfollow actions. The more recent action takes priority.

### Profile merge (kind 0)

Per-field merge using the `content` JSON object:

1. Parse ancestor, A, and B profile JSON
2. For each field: if only one side changed it, that value wins
3. If both sides changed the same field, **later timestamp wins**

Fields are compared independently — A changing `display_name` and B changing `about` is not a conflict.

### Merge execution

Merge happens automatically on whichever device detects the fork first. No user prompt. The merging device publishes the merged result as a new event with `prev` pointing to both fork tips.

### Merge is idempotent

If multiple devices detect the same fork and both merge, they produce the same result (deterministic rules). The later-timestamped merge simply replaces the earlier one per standard replaceable event semantics.

## Consequences

- Fork detection is reliable — `prev` tag creates a causal chain
- Follow list changes are never silently lost
- Profile updates from different devices merge cleanly
- No user intervention needed — merge is automatic and deterministic
- Small protocol addition: one `prev` tag on two event kinds
- Merge logic lives in the client — relays are unaware
- Compatible with standard Nostr relays (they ignore unknown tags)
