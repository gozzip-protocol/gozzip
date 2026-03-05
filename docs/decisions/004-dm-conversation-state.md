# ADR 004: DM Conversation State (Kind 10052)

**Date:** 2026-02-28
**Status:** Accepted

## Context

DMs between devices are append-only — two messages sent simultaneously from different devices aren't a conflict, just two messages ordered by `created_at`. The real multi-device problem is **read-state sync**: if you read a conversation on your phone, your desktop should know it's been read.

Without read-state sync, every device shows its own unread count. Switching between phone and laptop means constantly dismissing the same "unread" conversations. This breaks the one-account illusion.

Read state can't live in the DM events themselves (they're immutable and addressed to the conversation partner, not to yourself). It needs a separate, self-addressed event.

## Decision

Introduce **kind 10052 (conversation state)** — a parameterized replaceable event encrypted to the user's own root pubkey. It tracks `read_until` (timestamp) per conversation partner.

### Schema

```json
{
  "kind": 10052,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["d", "<conversation_partner_root_pubkey>"],
    ["read_until", "<unix_timestamp>"]
  ],
  "content": "<NIP-44 encrypted to own root pubkey>",
  "sig": "<signed by device key>"
}
```

- **Parameterized replaceable** — one event per conversation partner (keyed by `d` tag)
- **Encrypted content** — the `read_until` timestamp is also stored encrypted in `content` for privacy (tags may be visible to relays). Clients use the encrypted value; the tag is a hint for relay-side filtering.
- **Self-addressed** — encrypted to the user's own root pubkey, so all devices can decrypt

### How it works

1. User reads conversation with Bob on mobile
2. Mobile publishes kind 10052 with `d` = Bob's pubkey, `read_until` = timestamp of last read message
3. Desktop fetches kind 10052 for Bob's conversation → sees `read_until` → marks messages up to that point as read
4. If desktop had already published a later `read_until`, the later timestamp wins (standard replaceable event semantics)

### Conflict resolution

`read_until` only moves forward. If two devices publish kind 10052 for the same conversation, the one with the later `read_until` timestamp is correct. Standard replaceable event semantics (latest event replaces earlier) handles this naturally, since reading more of a conversation always produces a later timestamp.

## Consequences

- Read state syncs across all devices automatically
- One event per conversation — bounded storage (not per-message)
- Encrypted — relays can't see who you're talking to or how far you've read
- No new protocol concepts — uses existing parameterized replaceable event pattern
- `read_until` is monotonically increasing — no conflict possible
- Clients that don't support kind 10052 simply ignore it (graceful degradation)
- Future extensibility: the encrypted content can carry additional state (typing indicators, draft text) without schema changes
