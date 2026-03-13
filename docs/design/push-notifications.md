# Push Notifications

How Gozzip delivers real-time notifications on mobile without exposing message content to any intermediary.

## Problem

Mobile operating systems kill background processes aggressively. iOS BGAppRefreshTask and Android Doze limit true background uptime to 0.3-5%. Without push notifications, the app is a polling client that users must manually open. This is fatal for a messaging use case.

Push notifications require Apple (APNs) and Google (FCM) infrastructure. This is a platform reality, not a protocol compromise. Every messaging app -- including Signal -- uses these channels for wake-up delivery.

## Design

### Architecture

```
Event published ──► Relay ──► Notification Relay ──► APNs/FCM ──► Device
                                                         │
                                                    "sync now"
                                                    (no content)
                                                         │
                                                    App wakes ──► Syncs from relays/pact partners
                                                         │
                                                    Local notification with actual content
```

### Notification Relay

A notification relay is a lightweight service that:

1. Subscribes to standard Nostr relays on behalf of registered users
2. Watches for events matching the user's filter preferences (DMs, mentions, replies)
3. Sends a minimal "sync now" push via APNs/FCM when a matching event arrives
4. Holds no message content -- the push payload is empty or contains only a signal type (e.g., `{"t": "dm"}`)

Notification relays are independent services. Anyone can run one. Users register with 1-3 notification relays for redundancy -- if one goes down, others still deliver.

### Privacy Model

**What the notification relay knows:**
- The user's pubkey (necessary to subscribe on their behalf)
- That the user registered for push notifications
- That a matching event arrived (timing metadata)
- The user's encrypted push token (cannot read it without the relay's own key)

**What the notification relay does NOT know:**
- Message content (push payload contains no content)
- Who sent the message (the relay sees the event but the push contains no sender info)
- The full message after the app wakes (app syncs directly from relays/pact partners)

**What Apple/Google see:**
- A "sync now" signal was delivered to a device
- No message content, no sender, no metadata beyond "something happened"

This is the same privacy model Signal uses for push delivery.

### Registration Event: Kind 10062

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
    \"platform\": \"ios|android\",
    \"filters\": {
      \"dm\": true,
      \"mention\": true,
      \"reply\": true,
      \"reaction\": false
    }
  }>",
  "sig": "<signed by device key>"
}
```

- **Parameterized replaceable** -- `d` tag keys one registration per notification relay. Publishing a new 10062 with the same `d` tag replaces the previous registration (token rotation, filter updates).
- **Encrypted content** -- the push token and filter preferences are NIP-44 encrypted to the notification relay's pubkey. Only the notification relay can decrypt the token. Relays storing this event see an opaque blob.
- **Filter preferences** -- the user specifies which event types trigger a push. Reduces unnecessary wake-ups.
- **Platform field** -- `ios` (APNs) or `android` (FCM). The notification relay needs this to select the correct push channel.
- **Token rotation** -- when the OS rotates the push token, the client publishes a new 10062 with the updated token.
- **Deregistration** -- publish a 10062 with empty content to deregister. The notification relay drops the subscription.

### Notification Relay Behavior

1. On receiving a kind 10062 registration, the relay decrypts the content, extracts the push token and filters
2. The relay subscribes to the user's events on their NIP-65 relays (or relays specified in the registration)
3. When a matching event arrives:
   - DM (kind 14) addressed to the user
   - Event with a `p` tag mentioning the user (mention)
   - Reply to one of the user's events (reply thread)
4. The relay sends a push notification via APNs/FCM:
   ```json
   {
     "content-available": 1,
     "t": "dm"
   }
   ```
5. The push is a silent/background notification -- no visible alert. The app wakes, syncs from relays and pact partners, then generates a local notification with the actual message content.

### Deduplication

When a user registers with multiple notification relays, the same event may trigger pushes from all of them. The client deduplicates on wake:

- Track the last sync timestamp
- On wake from push, sync events since last timestamp
- Generate local notifications only for genuinely new events
- Multiple wake-ups for the same event are harmless (the sync is idempotent)

### Multiple Notification Relays

Users SHOULD register with 2-3 notification relays for redundancy:

- If one relay goes offline, others still deliver
- Different relays can be run by different operators (trust distribution)
- The client rotates which relay it checks first for health monitoring
- Notification relays can be community-operated, relay-operator-bundled, or self-hosted

### Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Notification relay goes down | No push until restored | Multiple relay registrations |
| Push token expires | Pushes fail silently | Client re-registers on app open |
| APNs/FCM outage | No push delivery | App still syncs on manual open |
| Notification relay is malicious | Can suppress pushes (DoS) but cannot read content | Multiple relays; user notices missing notifications and switches |

### Relationship to Existing Architecture

- Notification relays are separate from storage relays and standard Nostr relays
- They do not store events -- they monitor and forward wake-up signals
- They interact with the standard relay layer (subscribing to events) but add no protocol requirements to relays
- Compatible with the tiered retrieval cascade -- the app wakes and uses the standard sync path (cached endpoints, gossip, relay fallback)
- Kind 10062 follows the same conventions as other Gozzip events: `protocol_version` tag, signed by device key, `root_identity` tag for device-signed events

### Operational Considerations

Running a notification relay requires:

- Apple Developer account (for APNs certificates) and/or Google Firebase project (for FCM)
- Persistent WebSocket connections to Nostr relays
- Modest compute -- the relay is a filter + push sender, not a data store
- Rate limiting on push delivery to avoid APNs/FCM throttling

This is similar in complexity to running a Nostr relay. Community operators, relay operators, or the Gozzip project itself can run notification relays during bootstrap.
