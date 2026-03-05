# Multi-Device Sync

How multiple devices stay in sync without a central coordinator. The model is fork-and-reconcile: devices are peers, not leader/follower.

## The Problem

A user has a phone and a laptop. Both can post, react, DM, follow. There is no server deciding ordering. Both devices may act simultaneously while the other is offline.

The user must never notice. It must feel like one account, one feed, one identity — like Twitter.

## Event Categories

Not all events need the same sync strategy:

| Category | Kinds | Conflict possible? | Strategy |
|----------|-------|-------------------|----------|
| Append-only | 1, 6, 7, 9, 14, 30023 | No | Merge by timestamp |
| Replaceable | 0, 3 | Yes | `prev` tag + automatic merge |
| Device metadata | 10050, 10051 | No | Intentional updates only |
| Read state | 10052 | No | Monotonic — latest wins |

## Checkpoint Reconciliation

The checkpoint (kind 10051) is the reconciliation mechanism. There is no re-wrapping or event duplication. See [ADR 002](../decisions/002-checkpoint-reconciliation.md).

### Algorithm

```
Device A comes online:

1. Fetch kind 10050 → get sibling device pubkeys
2. Fetch latest kind 10051 → get last known heads
3. Query relay for events from sibling devices since last known heads
4. Publish new kind 10051 with updated heads (requires checkpoint_delegate authorization)
```

### Example

```
Device A (mobile)                     Device B (desktop)
  │                                     │
  ├─ Posts event A1                     ├─ Posts event B1
  ├─ Posts event A2                     ├─ Posts event B2
  │                                     │
  │  ── comes online, sees B1, B2 ──    │
  │                                     │
  ├─ Publishes checkpoint:              │
  │    device A head = A2               │
  │    device B head = B2               │
  │    merkle_root = H(A1,A2,B1,B2), n=4 │
  │                                     │
  └─ Done. No duplication.              └─ Sees checkpoint on next sync.
```

Observers (other users' clients) see A1, A2, B1, B2 as one timeline, ordered by `created_at`. The checkpoint tells light nodes where each device's chain is at.

### Eager reconciliation

Reconciliation is eager — a device publishes an updated checkpoint as soon as it comes online and discovers new sibling events. This keeps the checkpoint fresh for light nodes.

### Long-offline devices

A device offline for days or weeks simply publishes a checkpoint reflecting the current state. It does not replay intermediate states — the checkpoint only tracks heads, not history.

## Per-Event Hash Chain

Between checkpoints (~monthly), individual events are verifiable via a per-device hash chain. Each event references the previous event from the same device:

```json
{
  "kind": 1,
  "pubkey": "<device_pubkey>",
  "tags": [
    ["root_identity", "<root_pubkey>"],
    ["seq", "47"],
    ["prev_hash", "<H(previous_event_id)>"]
  ]
}
```

- `seq` — monotonically increasing sequence number per device, starting at 0
- `prev_hash` — hash of the previous event ID from this device
- First event: `seq` = 0, `prev_hash` omitted

**Completeness verification becomes per-event:**
- Gap in sequence numbers → missing event
- `prev_hash` mismatch → tampered or reordered chain
- No need to wait for checkpoint to detect withholding

The checkpoint Merkle root provides cross-device verification (proves a publisher saw all devices' events). The per-event chain handles single-device stream completeness.

### seq Counter Recovery

If a device is reinstalled or set up fresh with the same device key, the local `seq` counter resets to 0. This breaks the per-event hash chain. Recovery procedure (client-side, no protocol change). See [ADR 008](../decisions/008-protocol-hardening.md).

1. On device initialization, before publishing any events, query the last known `seq` from peers and relays
2. Fetch the most recent event with `pubkey = device_pubkey` and read the `seq` tag
3. Resume from `last_known_seq + 1`
4. If no events are found (truly new device), start at 0

### Checkpoint Cross-Verification

The checkpoint Merkle root alone is not sufficient — a compromised checkpoint delegate could publish a checkpoint that omits events from a sibling device. Light nodes add a cross-verification step. See [ADR 008](../decisions/008-protocol-hardening.md).

- After fetching the checkpoint, verify the per-event hash chain for the most recent M events (default: M=20) from each device listed in the checkpoint
- If the per-event chain shows events not reflected in the checkpoint heads, the checkpoint is inconsistent
- Inconsistent checkpoints trigger a user alert and fallback to alternative sources (other storage peers, relays)

## Bounded Timestamps

Device clocks can be manipulated. Setting a clock to the future wins every "later timestamp wins" merge conflict permanently. Validation rules prevent this.

**Validation (enforced by relays, storage peers, and clients):**
- Reject events with `created_at` more than 15 minutes in the future
- Flag events backdated more than 1 hour from the last known event from the same device

**Merge tiebreaker for replaceable events:**
```
if abs(timestamp_A - timestamp_B) < 60 seconds:
  → lexicographic ordering of event ID (deterministic, non-gameable)
else:
  → later timestamp wins (existing rule)
```

Event IDs are hashes — effectively random. Neither side can predict or game the tiebreaker.

No protocol changes — validation rules and tiebreaker are client/relay-side logic.

## Replaceable Event Merge

Kind 0 (profile) and kind 3 (follow list) can fork when two devices update them independently. See [ADR 003](../decisions/003-replaceable-event-merge.md).

### Fork Detection

Every kind 0 and kind 3 event includes a `prev` tag pointing to the event it replaces:

```json
["prev", "<event_id_of_replaced_event>"]
```

A fork exists when two events share the same `prev` value.

### Follow List Merge Algorithm

```
Given: ancestor (common prev), version A, version B

added_by_A  = A.tags - ancestor.tags
added_by_B  = B.tags - ancestor.tags
removed_by_A = ancestor.tags - A.tags
removed_by_B = ancestor.tags - B.tags

merged = ancestor
       + added_by_A
       + added_by_B
       - removed_by_A
       - removed_by_B

Conflict (in added_by_A ∩ removed_by_B, or vice versa):
  → most recent action wins (compare created_at of the kind 3 events)
```

**Example:** Ancestor follows [Alice, Bob, Carol]. Device A adds Dave, removes Carol. Device B adds Eve, removes Bob. Merged: [Alice, Dave, Eve]. No conflict — removals don't overlap with additions.

**Conflict example:** Device A follows Mallory (kind 3 at t=100). Device B unfollows Mallory (kind 3 at t=200). Conflict → most recent action wins → Mallory is removed (unfollow at t=200 is later). See [ADR 008](../decisions/008-protocol-hardening.md).

### Profile Merge Algorithm

```
Given: ancestor JSON, version A JSON, version B JSON

For each field in union(A.fields, B.fields):
  if field changed in A only → use A's value
  if field changed in B only → use B's value
  if field changed in both  → use value from later timestamp
  if field unchanged         → keep ancestor value
```

### Merge Execution

- Automatic — whichever device detects the fork first publishes the merged result
- Deterministic — multiple devices merging the same fork produce the same output
- No user prompt — merge is silent

## DM Ordering and Read-State Sync

### Message ordering

DMs (kind 14) are append-only. Two messages from different devices aren't a conflict — they're just two messages. Order by `created_at`.

### Read-state sync

The real multi-device DM problem is read-state: if you read a conversation on your phone, your desktop should know. See [ADR 004](../decisions/004-dm-conversation-state.md).

Kind 10052 (conversation state) tracks `read_until` per conversation partner:

```
User reads conversation with Bob on mobile
  │
  ├─ Mobile publishes kind 10052:
  │    d = Bob's pubkey
  │    read_until = timestamp of last read message
  │    content = encrypted to own root pubkey
  │
  └─ Desktop fetches 10052 → marks Bob's conversation as read
```

- One event per conversation (parameterized replaceable, keyed by `d` tag)
- `read_until` only moves forward — no conflict possible
- Encrypted to own root pubkey — all devices can decrypt, relays can't read

## What the User Sees

Nothing. The app shows a unified feed. If they post from their phone, it appears on their desktop. If they follow someone from their laptop, the phone sees it on next sync. Read/unread state follows them across devices. Exactly like Twitter.
