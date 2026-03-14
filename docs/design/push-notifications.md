# Push Notification Design
**Date:** 2026-03-14
**Status:** Draft

## Problem

Mobile operating systems kill background processes aggressively. iOS BGAppRefreshTask and Android Doze limit true background uptime to 0.3-5%. Without push notifications, the app is a polling client that users must manually open to check for messages. This is fatal for a messaging use case.

This is the single most important UX gap versus Signal, WhatsApp, and Telegram. Users will not adopt a messaging app that requires manual polling. Push notifications are a hard prerequisite for mobile viability.

Push notifications require Apple (APNs) and Google (FCM) infrastructure. This is a platform reality, not a protocol compromise. Every messaging app -- including Signal -- uses these channels for wake-up delivery. Using APNs/FCM does not undermine decentralization because the notification is a wake-up signal, not a data channel.

## Architecture

```
Event published --> Relay --> Notification Relay --> APNs/FCM/UnifiedPush/Web Push --> Device
                                                         |
                                                    "sync now"
                                                    (no content)
                                                         |
                                                    App wakes --> Syncs from relays/pact partners
                                                         |
                                                    Local notification with actual content
```

The notification relay is a dedicated service that bridges Nostr's event subscription model to platform push notification infrastructure. It subscribes to events on behalf of registered mobile clients and sends opaque wake-up signals when relevant events arrive.

## 1. Notification Relay Service

A notification relay is a lightweight, dedicated service role with a single purpose: watch for events matching a registered user's filter preferences and send a minimal push notification via platform channels.

### Responsibilities

1. Accept kind 10062 registration events from mobile clients (authenticated via NIP-46 session)
2. Subscribe to standard Nostr relays on behalf of registered users
3. Watch for events matching the user's filter preferences (DMs, mentions, replies)
4. Send a minimal "sync now" push via APNs, FCM, UnifiedPush, or Web Push when a matching event arrives
5. Hold no message content -- the push payload is empty or contains only a signal type

### NIP-46 Authentication

Mobile clients register with notification relays via NIP-46 relay-mediated encrypted sessions:

1. Client discovers notification relay via kind 10067 advertisement (see below) or manual configuration
2. Client establishes NIP-46 session with the notification relay's pubkey
3. Client publishes kind 10062 registration event encrypted to the notification relay
4. Notification relay decrypts the registration, extracts the push token and filters
5. Notification relay begins subscribing to the user's events on their NIP-65 relays

The NIP-46 session provides mutual authentication: the client verifies the notification relay's identity, and the relay verifies the client's identity via the signed registration event.

### Subscription Behavior

1. On receiving a kind 10062 registration, the relay decrypts the content and extracts the push token, platform, and filter preferences
2. The relay subscribes to the user's events on their NIP-65 relays (or relays specified in the registration)
3. When a matching event arrives:
   - DM (kind 14) addressed to the user
   - Event with a `p` tag mentioning the user (mention)
   - Reply to one of the user's events (reply thread)
   - Reaction to one of the user's events (if enabled)
4. The relay sends a push notification via the appropriate platform channel:
   ```json
   {
     "content-available": 1,
     "t": "dm"
   }
   ```
5. The push is a silent/background notification -- no visible alert. The app wakes, syncs from relays and pact partners, then generates a local notification with the actual message content.

### Deregistration

Publish a kind 10062 with empty content to the same `d` tag. The notification relay drops the subscription and deletes the push token.

## 2. Provider Support

### Apple Push Notification service (APNs) -- iOS

- Platform identifier: `ios`
- Push type: `background` (content-available: 1) for silent wake-up
- Payload: `{"aps": {"content-available": 1}, "t": "dm"}` -- under the 4 KB APNs limit
- Requires: Apple Developer account ($99/year), APNs certificate or key
- Rate limiting: Apple throttles background pushes aggressively -- batch if sending more than ~3/hour
- The app registers for remote notifications and provides the device token to the notification relay via kind 10062

### Firebase Cloud Messaging (FCM) -- Android

- Platform identifier: `android`
- Push type: data message (no notification payload) for silent wake-up
- Payload: `{"data": {"t": "dm"}}` -- triggers WorkManager processing
- Requires: Firebase project (free tier sufficient for push)
- Rate limiting: FCM priority `normal` for non-urgent, `high` for DMs
- Google-dependent -- requires Google Play Services on the device

### UnifiedPush -- De-Googled Android

- Platform identifier: `unifiedpush`
- For devices without Google Play Services (LineageOS, GrapheneOS, CalyxOS)
- Client registers a UnifiedPush endpoint URL instead of an FCM token
- Notification relay sends HTTP POST to the endpoint URL with the same minimal payload
- Self-hosted option: user runs their own UnifiedPush distributor (ntfy, Gotify, NextPush)
- No Google dependency -- the sovereignty-aligned option
- Registration event content includes `"push_endpoint": "https://ntfy.example.com/gozzip-abc123"` instead of a platform token

### Web Push (VAPID) -- Browser Extensions

- Platform identifier: `web`
- Uses the Web Push Protocol (RFC 8030) with VAPID authentication (RFC 8292)
- Client provides a Web Push subscription object (endpoint URL + keys)
- Notification relay encrypts the payload per Web Push encryption spec (RFC 8291)
- Works with browser extensions and PWAs
- No app store dependency
- Registration event content includes the full Web Push subscription object

### Platform Selection Matrix

| Platform | Token type | Dependency | Cost | Self-hostable |
|----------|-----------|------------|------|---------------|
| APNs | Device token | Apple | $99/year | No (Apple infra) |
| FCM | Registration token | Google | Free | No (Google infra) |
| UnifiedPush | Endpoint URL | None | Free | Yes |
| Web Push | Subscription object | None | Free | Yes |

## 3. Privacy-Preserving Design

### What the Notification Relay Knows

- The user's pubkey (necessary to subscribe on their behalf)
- That the user registered for push notifications
- Which event kinds the user wants push notifications for (DMs, mentions, etc.)
- That a matching event arrived (timing metadata -- "something happened at time T")
- The user's encrypted push token (relay decrypts it to deliver the push)

### What the Notification Relay Does NOT Know

- Message content (push payload contains no content; events are NIP-44 encrypted for DMs)
- Who sent the message (the relay sees the event but the push payload contains no sender info)
- The full message after the app wakes (app syncs directly from relays/pact partners)

### What Apple/Google See

- A "sync now" signal was delivered to a device
- No message content, no sender, no metadata beyond "something happened"

This is the same privacy model Signal uses for push delivery.

### Push Payload Opacity

The push payload is intentionally opaque. It contains only:

- A signal type (`dm`, `mention`, `reply`, `reaction`) -- tells the app which sync path to prioritize
- A `content-available` flag to trigger background processing

No content, no sender pubkey, no event ID, no relay URL. The app wakes and performs a full sync from its configured relay/pact partner sources.

### Token Rotation

Push tokens are a tracking vector. The OS may rotate them, and the notification relay holds them. Mitigations:

1. **OS-initiated rotation:** When the OS rotates the push token (common on both iOS and Android), the client publishes a new kind 10062 with the updated token. The old token becomes invalid.
2. **Client-initiated rotation:** Clients SHOULD re-register with a fresh kind 10062 periodically (every 30 days recommended) even if the OS token hasn't changed. This limits the tracking window for a notification relay that logs token-to-pubkey associations over time.
3. **Token encryption:** The push token in kind 10062 is NIP-44 encrypted to the notification relay's pubkey. Other relays storing the event see only an opaque blob. Only the intended notification relay can extract the token.

### Metadata Exposure Mitigation

The notification relay learns timing metadata: "user X received an event matching filter Y at time T." This is the fundamental privacy cost of push notifications. Mitigations:

1. **Multiple notification relays with overlapping coverage:** Register with 2-3 independent notification relays. Each sees the same timing data, but no single operator has exclusive visibility. If one relay is compromised, the user can deregister and switch.
2. **Filter minimization:** Register only for DM notifications (the most time-sensitive). Mentions, replies, and reactions can wait for the next foreground sync.
3. **Notification relay diversity:** Use relays operated by different entities in different jurisdictions to limit correlation.

## 4. Event Filtering

### Client-Configured Filters

The client specifies which event kinds trigger a push notification in the kind 10062 registration:

```json
{
  "dm": true,
  "mention": true,
  "reply": false,
  "reaction": false
}
```

- **DMs (kind 14):** Always recommended. DMs are the most time-sensitive event type.
- **Mentions:** Optional. Events with a `p` tag matching the user's pubkey.
- **Replies:** Optional. Events that are direct replies to the user's events (via `e` tag threading).
- **Reactions:** Optional and generally discouraged -- reactions generate high-volume, low-urgency pushes.

### Server-Side Subscription Filters

The notification relay translates filter preferences into NIP-01 REQ filters on the user's NIP-65 relays:

```json
[
  {"#p": ["<user_pubkey>"], "kinds": [14]},
  {"#p": ["<user_pubkey>"], "kinds": [1, 7], "since": <now>}
]
```

This uses standard Nostr relay subscription semantics -- no custom relay support required.

### Rate Limiting and Batching

To avoid APNs/FCM throttling and excessive battery drain:

- **Batching window:** If more than 3 events match within a 30-second window, batch them into a single push notification. The push payload becomes `{"t": "batch", "n": 7}` -- the app wakes once and syncs all pending events.
- **Cooldown period:** After sending a push, enforce a 60-second cooldown before sending another to the same device. Events arriving during cooldown are queued and sent as a batch when cooldown expires.
- **Daily cap:** Maximum 100 push notifications per device per day. Beyond this, the notification relay stops pushing until the next UTC day. This prevents a spam attack from draining the user's battery via constant wake-ups.

### Quiet Hours (Do Not Disturb)

Client-configured quiet hours are included in the kind 10062 registration:

```json
{
  "token": "<apns_token>",
  "platform": "ios",
  "filters": {"dm": true, "mention": true, "reply": false, "reaction": false},
  "quiet": {"start": "23:00", "end": "07:00", "tz": "UTC+9"}
}
```

- During quiet hours, the notification relay queues matching events and delivers a single batch push when quiet hours end
- The timezone offset lets the notification relay compute the user's local time without knowing their location (the offset, not the timezone name)
- DM notifications can optionally bypass quiet hours with `"dm_bypass_quiet": true`

## 5. Trust Model

### Notification Relay as Trusted Third Party

The notification relay occupies a specific position in the trust model:

**It can:**
- Suppress notifications (denial of service -- user doesn't get woken up)
- Learn timing metadata (when events arrive for the user)
- Correlate push token with pubkey (knows which device belongs to which identity)

**It cannot:**
- Read message content (NIP-44 encrypted)
- Forge events (it doesn't have the user's signing keys)
- Modify events in transit (events are signed by the author)
- Impersonate the user (push is one-way: relay to device)

### Deployment Models

**Self-hosted notification relay** -- maximum privacy:
- User runs their own notification relay on a VPS, Raspberry Pi, or home server
- Requires APNs certificate (Apple Developer account) and/or Firebase project setup
- The user trusts only their own infrastructure
- Suitable for technical users and organizations

**Community-run notification relays** -- middle ground:
- Operated by relay operators, community organizations, or Nostr ecosystem projects
- Trust is social: users choose relays operated by people or organizations they trust
- Comparable to choosing a Nostr relay or email provider
- Multiple community relays provide redundancy and competitive trust dynamics

**Project-operated notification relay** -- bootstrap phase:
- During initial deployment, the Gozzip project operates 1-2 notification relays as a default
- Provides a baseline for users who haven't chosen their own relays
- Clearly documented as a convenience, not a requirement
- Users are encouraged to add community or self-hosted relays alongside the default

### Multiple Relays for Redundancy

Users SHOULD register with 2-3 notification relays:

- If one relay goes offline, others still deliver wake-up signals
- Different relays operated by different entities distribute trust
- The client rotates which relay it checks first for health monitoring
- Deduplication is handled client-side (see below)

## 6. Integration with Pact Layer

### Pact Partners as Notification Proxies

Full-node pact partners are naturally positioned to serve as notification proxies for their mobile pact peers:

1. Pact partner's full node is already subscribed to the mobile user's events (pact requirement)
2. When the pact partner receives an event destined for the offline mobile peer, it can trigger a push via the notification relay
3. This reduces the notification relay's subscription burden -- instead of subscribing to all of the user's NIP-65 relays, the notification relay can subscribe only to the pact partner's relay

### Full-Node-to-Mobile Pact Notification Flow

```
Sender --> Relay --> Pact Partner (full node) --> Notification Relay --> APNs/FCM --> Mobile Device
                            |                                                            |
                     Stores event                                                   Wakes, syncs
                     for mobile peer                                                from pact partner
```

When a pact partner receives an event for their mobile peer:

1. The pact partner checks if the event matches the mobile peer's notification preferences (exchanged during pact formation)
2. If it matches, the pact partner publishes a lightweight "wake-up hint" (ephemeral kind 20062) to the notification relay
3. The notification relay sends the push
4. The mobile device wakes and syncs directly from the pact partner

This reduces the notification relay's role from "event monitor" to "push delivery service," minimizing its metadata exposure. The pact partner already knows about the events (it stores them), so no new metadata is leaked.

### Reducing Notification Relay Dependency

Users with full-node pact partners (desktop machines, home servers, VPS) can configure their pact partners as the primary event watchers:

- The notification relay only needs to hold the push token and deliver pushes
- Event filtering and matching happens at the pact partner level
- If all pact partners go offline simultaneously, the notification relay falls back to direct relay subscriptions

This creates a graceful degradation path: pact partner notification proxy (best privacy) --> notification relay with direct subscriptions (standard) --> background fetch polling (worst case, 15-30 minute intervals).

## 7. Event Kinds

### Kind 10062 -- Push Notification Registration

Parameterized replaceable event -- one per notification relay. The user publishes this to register their push token with a notification relay.

```json
{
  "kind": 10062,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["d", "<notification_relay_pubkey>"],
    ["relay", "wss://notify.example.com"],
    ["protocol_version", "1"]
  ],
  "content": "<NIP-44 encrypted to notification_relay_pubkey: {
    \"token\": \"<apns_or_fcm_token>\",
    \"platform\": \"ios|android|unifiedpush|web\",
    \"push_endpoint\": \"<url, for unifiedpush/web only>\",
    \"filters\": {
      \"dm\": true,
      \"mention\": true,
      \"reply\": false,
      \"reaction\": false
    },
    \"quiet\": {
      \"start\": \"23:00\",
      \"end\": \"07:00\",
      \"tz\": \"UTC+9\",
      \"dm_bypass_quiet\": true
    },
    \"batch\": {
      \"window_seconds\": 30,
      \"cooldown_seconds\": 60
    }
  }>",
  "sig": "<signed by device key>"
}
```

**Fields:**

- **Parameterized replaceable** -- `d` tag keys one registration per notification relay. Publishing a new 10062 with the same `d` tag replaces the previous registration (token rotation, filter updates).
- **Encrypted content** -- the push token and all preferences are NIP-44 encrypted to the notification relay's pubkey. Only the notification relay can decrypt the token. Relays storing this event see an opaque blob.
- **Platform** -- `ios` (APNs), `android` (FCM), `unifiedpush`, or `web` (VAPID). The notification relay uses this to select the correct push channel.
- **push_endpoint** -- for `unifiedpush` and `web` platforms, the HTTP endpoint URL to POST the push payload to. Not used for `ios` or `android`.
- **Filter preferences** -- which event types trigger a push. Reduces unnecessary wake-ups.
- **Quiet hours** -- optional do-not-disturb window with timezone offset.
- **Batch settings** -- optional rate limiting configuration.
- **Token rotation** -- when the OS rotates the push token, the client publishes a new 10062 with the updated token.
- **Deregistration** -- publish a 10062 with empty content to deregister. The notification relay drops the subscription.

### Kind 10067 -- Notification Relay Advertisement

Replaceable event published by a notification relay operator to advertise the relay's capabilities and supported platforms.

```json
{
  "kind": 10067,
  "pubkey": "<notification_relay_operator_pubkey>",
  "tags": [
    ["d", "notification-relay"],
    ["relay", "wss://notify.example.com"],
    ["platform", "ios"],
    ["platform", "android"],
    ["platform", "unifiedpush"],
    ["platform", "web"],
    ["nip46_relay", "wss://relay.example.com"],
    ["protocol_version", "1"]
  ],
  "content": "<optional: JSON with terms of service, capacity limits, contact info>",
  "sig": "<signed by notification_relay_operator_key>"
}
```

**Fields:**

- **`relay` tag** -- WebSocket URL where the notification relay accepts NIP-46 sessions and kind 10062 registrations
- **`platform` tags** -- which push platforms the relay supports (multiple tags allowed)
- **`nip46_relay` tag** -- relay used for NIP-46 session establishment (may differ from the notification relay's own endpoint)
- **`content`** -- optional metadata: capacity limits, terms, operator contact info
- Clients discover notification relays by querying for kind 10067 events on well-known relays or via WoT recommendations

### Kind 20062 -- Wake-Up Hint (Ephemeral)

Ephemeral event published by a pact partner to signal the notification relay that a registered user has a pending event. Used in the pact-partner-as-notification-proxy flow.

```json
{
  "kind": 20062,
  "pubkey": "<pact_partner_device_pubkey>",
  "tags": [
    ["p", "<target_user_root_pubkey>"],
    ["t", "dm"],
    ["root_identity", "<pact_partner_root_pubkey>"]
  ],
  "content": "",
  "sig": "<signed by pact partner device key>"
}
```

**Fields:**

- **Ephemeral** -- kind 20000-29999, not stored by relays
- **`p` tag** -- the user who should receive the push notification
- **`t` tag** -- the notification type (dm, mention, reply, reaction)
- The notification relay verifies that the sender is a known pact partner of the target user (prevents arbitrary wake-up spam)
- No content, no event ID, no sender of the original event -- minimal metadata

## 8. Deduplication

When a user registers with multiple notification relays, the same event may trigger pushes from all of them. The client handles this:

- Track the last sync timestamp per source
- On wake from push, sync events since last timestamp
- Generate local notifications only for genuinely new events
- Multiple wake-ups for the same event are harmless (the sync is idempotent)
- The notification type tag (`t`) helps the client prioritize which sync to perform first (DMs before mentions)

## 9. Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Notification relay goes down | No push until restored | Multiple relay registrations (2-3) |
| Push token expires | Pushes fail silently | Client re-registers on app open |
| APNs/FCM outage | No push delivery | App still syncs on manual open; UnifiedPush as backup |
| Notification relay is malicious | Can suppress pushes (DoS) but cannot read content | Multiple relays; user notices missing notifications and switches |
| Notification relay logs metadata | Timing correlation of events | Multiple relays, token rotation, filter minimization |
| Pact partner proxy fails | Falls back to direct relay subscription | Notification relay maintains both paths |
| Push rate throttled by platform | Delayed or dropped notifications | Batching and cooldown in kind 10062 settings |

## 10. Implementation Considerations

### Platform Requirements

- **APNs:** Requires Apple Developer account ($99/year) for APNs certificates or keys. The notification relay operator must be enrolled in the Apple Developer Program. Each app bundle ID needs its own APNs configuration.
- **FCM:** Requires a Google Firebase project (free tier). The notification relay operator creates a Firebase project and obtains server credentials. Free but Google-dependent.
- **UnifiedPush:** No platform account required. The notification relay sends HTTP POSTs to the user's self-hosted endpoint. This is the sovereignty-aligned option -- no corporate dependency, fully self-hostable.
- **Web Push:** Requires generating a VAPID key pair (free, no account needed). The notification relay signs push messages with the VAPID private key.

### Background Fetch as Partial Fallback

When push notifications are unavailable (user declined push permissions, notification relay down, no internet for push delivery):

- **iOS:** BGAppRefreshTask provides 15-30 minute polling intervals. The OS decides when to schedule the refresh based on user behavior patterns. Not reliable for time-sensitive messages but provides a baseline.
- **Android:** WorkManager with periodic work requests can poll every 15 minutes. More reliable than iOS but still not real-time.
- **Both platforms:** The client should attempt background fetch on every permitted interval, sync from relays/pact partners, and generate local notifications for new events.

Background fetch is a degraded fallback, not a replacement for push notifications. Users relying solely on background fetch will experience 15-30 minute delays in message delivery.

### Operational Requirements for Notification Relay Operators

Running a notification relay requires:

- Persistent WebSocket connections to Nostr relays (one subscription set per registered user)
- APNs certificates and/or Firebase server credentials
- Modest compute -- the relay is a filter + push sender, not a data store
- Rate limiting on push delivery to avoid platform throttling
- Token storage (in-memory or lightweight database) -- only push tokens, not events
- NIP-46 session handling for authenticated registration

This is similar in complexity to running a Nostr relay. Community operators, relay operators, or the Gozzip project itself can run notification relays during bootstrap.

### Connection Scaling

A notification relay maintaining subscriptions for N registered users needs approximately N WebSocket connections to Nostr relays (assuming each user's NIP-65 list overlaps partially). In practice, connection multiplexing across shared relays reduces this significantly -- if 1000 users all use the same 5 popular relays, the notification relay needs 5 connections, not 5000.

## 11. Relationship to Existing Architecture

- Notification relays are separate from storage relays and standard Nostr relays
- They do not store events -- they monitor and forward wake-up signals
- They interact with the standard relay layer (subscribing to events) but add no protocol requirements to relays
- Compatible with the tiered retrieval cascade -- the app wakes and uses the standard sync path (cached endpoints, gossip, relay fallback)
- Kind 10062 follows the same conventions as other Gozzip events: `protocol_version` tag, signed by device key, root_identity tag for device-signed events
- Kind 10067 enables decentralized relay discovery without a central registry
- Kind 20062 integrates push delivery with the existing pact layer, reducing metadata exposure for users with full-node pact partners
