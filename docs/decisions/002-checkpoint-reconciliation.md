# ADR 002: Checkpoint Reconciliation (No Re-wrapping)

**Date:** 2026-02-28
**Status:** Accepted

## Context

When multiple devices act on the same identity, they produce independent event chains. Eventually these chains must be reconciled so every device (and every observer) sees a unified timeline.

The only state that actually needs reconciliation is: "which events has each device seen?" Posts, reactions, and reposts are append-only — two devices posting simultaneously just produces two valid events. There is nothing to reconcile at the event level.

## Decision

No re-wrapping. The checkpoint (kind 10051) IS the reconciliation.

When device A comes online and sees device B's events, it publishes an updated kind 10051 reflecting B's head event ID and sequence number. That checkpoint update is the entire reconciliation. Individual events are never re-signed, duplicated, or wrapped.

### How it works

1. Device A comes online
2. Device A queries relay for events from sibling devices (via kind 10050 device list)
3. Device A sees device B has published events beyond the last known checkpoint
4. Device A publishes a new kind 10051 with updated heads for all devices
5. Done — reconciliation complete

### What about replaceable events?

Posts/reactions/reposts are append-only and need no reconciliation. Replaceable events (kind 0, kind 3) use a separate merge strategy — see [ADR 003](003-replaceable-event-merge.md).

### Eager reconciliation

Reconciliation is eager: a device publishes an updated checkpoint as soon as it comes online and discovers new sibling events. There is no lazy/deferred mode. This keeps the checkpoint fresh for light nodes that depend on it for sync.

### Long-offline devices

A device offline for an extended period simply publishes a checkpoint reflecting the current state when it reconnects. It does not need to replay or reconcile intermediate states — the checkpoint only tracks heads, not history.

## Rejected Alternative: Re-wrapping

Device A re-signs or duplicates device B's events into its own chain. Rejected because:

- **Event duplication** — every event stored N times (once per reconciling device), inflating storage and bandwidth
- **New event kinds** — requires a wrapper event kind, adding protocol complexity
- **Ambiguous ordering** — re-wrapped copy has a different timestamp and signature than the original, creating two "versions" of the same event

## Consequences

- No new event kinds needed for reconciliation
- No event duplication — each event exists exactly once
- Append-only semantics preserved for posts, reactions, reposts
- Checkpoints serve double duty: light-node sync AND device reconciliation
- Any checkpoint delegate can publish a checkpoint (authorized via kind 10050)
- Light nodes always have a fresh sync point after any device reconciles
- Reconciliation is O(1) — just publish one checkpoint event, regardless of gap size
