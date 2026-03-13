# Moderation Framework

Content moderation primitives for the Gozzip protocol. This is intentionally minimal -- the protocol provides primitives, not policy. Clients and communities decide what to filter.

## Design Principles

1. **No central authority** -- moderation is distributed across relay operators, labeling services, and individual users
2. **Author control** -- authors can request deletion of their own content (see GDPR Deletion below)
3. **Relay autonomy** -- relay operators set their own content policies, as they do in standard Nostr
4. **WoT as the primary filter** -- the 2-hop WoT boundary already prevents most spam from strangers
5. **Protocol provides tools, not rules** -- the protocol defines event kinds for reporting and labeling; what to do with them is up to clients and operators

## GDPR Deletion Request: Kind 10063

Authors can request deletion of their own events. This is a protocol-level mechanism for GDPR Article 17 ("right to erasure") compliance.

### Event Format

```json
{
  "kind": 10063,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["e", "<event_id_1>"],
    ["e", "<event_id_2>"],
    ["e", "<event_id_3>"],
    ["reason", "gdpr"],
    ["protocol_version", "1"]
  ],
  "content": "",
  "sig": "<signed by root key or governance key>"
}
```

- **`e` tags** -- reference specific event IDs to delete. One deletion request can cover multiple events.
- **`reason` tag** (optional) -- `gdpr`, `personal`, `error`, or omitted. Informational only -- deletion requests are valid regardless of reason.
- **Signed by root key or governance key** -- only the author can request deletion of their own events. Device keys cannot issue deletion requests (prevents a compromised device from wiping an identity's history).

### Deletion Semantics

**Who SHOULD honor deletion requests:**
- Pact partners storing the author's events
- Read-caches holding cached copies
- Relays storing the events
- Notification relays (if they retain any event data)

**Best-effort, not guaranteed:**
- Signed events may have been copied beyond the author's reach. Once an event is published, cryptographic signatures make it independently verifiable by anyone who holds a copy. Deletion requests cannot unsign an event.
- This is the honest reality of any decentralized system. GDPR acknowledges "best effort" deletion for distributed systems.
- The protocol provides the mechanism; compliance depends on participants honoring it.

**Enforcement via pact accountability:**
- Pact partners that systematically ignore deletion requests can be flagged and replaced. If a pact partner refuses to delete events after receiving a valid kind 10063, the author's client can:
  1. Log the refusal
  2. Drop the pact
  3. Replace the partner through standard pact renegotiation
  4. This is a natural consequence, not a punishment mechanism

**Relay behavior:**
- Standard Nostr relays already support NIP-09 deletion events (kind 5). Kind 10063 extends this concept with Gozzip-specific semantics (root/governance key signing, pact partner targeting).
- Relays MAY treat kind 10063 as equivalent to NIP-09 kind 5 for their own retention policies.

## Content Report: Kind 10064

Users can report content to relay operators and community moderators.

### Event Format

```json
{
  "kind": 10064,
  "pubkey": "<reporter_root_pubkey>",
  "tags": [
    ["e", "<reported_event_id>"],
    ["p", "<reported_author_pubkey>"],
    ["report", "<category>"],
    ["protocol_version", "1"]
  ],
  "content": "<optional description>",
  "sig": "<signed by device key>"
}
```

- **`e` tag** -- the event being reported
- **`p` tag** -- the author of the reported event (for relay operator routing)
- **`report` tag** -- category: `spam`, `harassment`, `illegal`, `csam`, `impersonation`, `other`
- **`content`** -- optional free-text description providing context

### Report Routing

Reports are published to:
1. The reporter's relays (standard event publication)
2. The reported author's relays (via NIP-65 relay list) so relay operators can act
3. Optionally, community moderator pubkeys (if the reporter is in a NIP-29 group or community with designated moderators)

Relay operators receive reports through their normal subscription flow. What they do with them is their decision -- the protocol does not mandate action.

### Handling Illegal Content (CSAM)

Pact partners storing potentially illegal content (specifically CSAM) face legal exposure. The protocol provides an explicit escape valve:

- If a pact partner receives a credible report of illegal content (kind 10064 with `report: csam` or `report: illegal`), they MAY:
  1. Delete the specific content immediately
  2. Drop the pact with no reliability penalty
  3. The dropped pact does not count against the partner's reliability score
- "Credible report" is deliberately undefined at the protocol level -- this is a judgment call for the pact partner's operator/user.
- Clients SHOULD treat pact drops with an accompanying kind 10064 `csam`/`illegal` report differently from unexplained pact drops -- no reliability score penalty.

This does not solve the problem of illegal content distribution. It gives well-intentioned participants a protocol-sanctioned way to protect themselves without being penalized by the pact system.

## Labeling Services (NIP-32 Compatible)

Third-party labeling services can tag content with labels that clients use for filtering. This is compatible with NIP-32 (Labeling).

### How It Works

1. A labeling service monitors events (subscribes to relays, runs classifiers, processes user reports)
2. It publishes NIP-32 label events (kind 1985) tagging content with labels:
   - `nsfw`, `spam`, `bot`, `satire`, `ai-generated`, etc.
3. Clients subscribe to labeling services they trust
4. Client-side filtering: show/hide content based on label preferences

### Integration with Gozzip

- Labeling services are just Nostr pubkeys that publish kind 1985 events -- no protocol changes needed
- Users choose which labeling services to subscribe to (WoT-based trust)
- Labels are advisory -- clients decide how to use them
- Multiple labeling services can label the same content differently (competing perspectives)
- Gozzip's WoT graph can inform label trust: labels from pubkeys within 2 hops are weighted higher

## WoT-Based Moderation

The existing WoT graph provides user-level moderation without new event kinds.

### Mute and Block

- **Mute** (NIP-51 kind 10000) -- hide content from a pubkey in the user's feed. Private (encrypted mute list).
- **Block** (client-side) -- refuse to serve data requests from a pubkey, do not form pacts with them.

### Mute Propagation

Mute lists can propagate as soft recommendations within the WoT:

- If 3+ users in your inner circle (direct follows) mute the same pubkey, the client MAY surface a suggestion: "3 people you follow have muted @spammer"
- This is a UI hint, not an automatic action -- the user decides whether to mute
- No new event kind needed -- clients compute this from existing mute lists (kind 10000) of followed users who have opted into public mute sharing

### Community-Level Moderation

For NIP-29 group chats and communities:
- Group admins can designate moderators via group metadata events (kind 39000-39009)
- Moderators can publish kind 10064 reports that carry more weight with relay operators
- Community relays can enforce community-specific rules (standard relay operator behavior)

## What This Framework Does NOT Do

- **Mandate content policy** -- the protocol is agnostic. Relay operators, communities, and users set their own standards.
- **Provide appeals** -- appeals are out of scope for the protocol. Communities and relay operators handle disputes.
- **Prevent all abuse** -- no system can. The framework provides tools to respond to abuse, not prevent it.
- **Guarantee deletion** -- deletion requests are best-effort in a decentralized system.
- **Define "illegal content"** -- jurisdiction-dependent. The protocol provides reporting and deletion primitives; legal interpretation is up to participants.
