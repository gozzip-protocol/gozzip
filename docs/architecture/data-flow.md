# Data Flow

How events travel through the Gozzip network.

## Publishing a Post

```
User (device key)
  │
  ├─ Signs kind 1 event with device subkey
  ├─ Adds root_identity tag pointing to root pubkey
  │
  └─► Relay
       └─ Stores event (standard Nostr relay, no modifications)
```

## Subscribing to a Feed

Client-side resolution (works with any standard Nostr relay):

```
Client subscribes to root_pubkey
  │
  ├─ Fetches kind 10050 for root_pubkey → [device_pubkey_1, device_pubkey_2]
  ├─ Subscribes to events from each device pubkey
  └─ Merges into unified feed attributed to root identity
```

Light node optimization — client uses checkpoint to skip known events:

```
Client subscribes to root_pubkey
  │
  ├─ Fetches kind 10051 (checkpoint) → device heads + sequences
  ├─ Subscribes to events AFTER each device's head
  └─ Returns only new events (last N, not full history)
```

**Optional relay optimization (oracle resolution):** An optimized relay can cache Kind 10050 mappings and index by `root_identity` tag, returning a unified feed in one round-trip instead of three. See [Relay](../actors/relay.md). This is an acceleration, not a requirement.

## Direct Message Flow

```
Sender (device key)
  │
  ├─ Encrypts content with NIP-44 to recipient's dm_key (from kind 10050)
  ├─ Wraps in NIP-59 sealed gift (metadata privacy)
  ├─ Signs outer event with device subkey
  │
  └─► Relay
       └─► Recipient
            ├─ All devices can decrypt (all hold derived DM privkey)
            └─ root_identity tag identifies sender
```

## Follow-as-Commitment Indexing

```
User A follows User B (kind 3 updated)
  │
  ├─ User A's node begins indexing User B's events
  ├─ User A's node indexes User B's connections (1st degree)
  │
  └─ Discovery beyond 1st degree:
       ├─ Possible through web of trust traversal
       └─ Not instant — requires relay queries
```

## Checkpoint Reconciliation Flow

```
Device A comes online
  │
  ├─ Fetches kind 10050 → [device_B_pubkey, device_C_pubkey]
  ├─ Fetches kind 10051 → last known heads per device
  │
  ├─ Queries relay: events from B, C since their last known heads
  │    └─ Gets B1, B2 (new from B), C1 (new from C)
  │
  ├─ Publishes new kind 10051 (checkpoint_delegate):
  │    device A head = A_latest
  │    device B head = B2
  │    device C head = C1
  │
  └─ Done. Observers see unified timeline ordered by created_at.
```

## Replaceable Event Fork Detection and Merge

```
Device A offline                     Device B offline
  │                                    │
  ├─ Updates kind 3 (follow list)      ├─ Updates kind 3 (follow list)
  │   prev = event_X                   │   prev = event_X         ← same ancestor
  │   adds [Dave]                      │   adds [Eve], removes [Bob]
  │                                    │
  ├─────── both come online ───────────┤
  │                                    │
  Device A (or B) detects fork:
    Two kind 3 events with prev = event_X
  │
  ├─ Computes merge:
  │    ancestor tags: [Alice, Bob, Carol]
  │    A's version:   [Alice, Bob, Carol, Dave]
  │    B's version:   [Alice, Carol, Eve]
  │    merged:        [Alice, Carol, Dave, Eve]
  │
  ├─ Publishes merged kind 3:
  │    prev = [A's event_id, B's event_id]   ← points to both fork tips
  │
  └─ All devices now see same follow list.
```

## Read-State Sync Flow

```
User reads DM conversation with Bob on mobile
  │
  ├─ Mobile publishes kind 10052:
  │    d = Bob's root pubkey
  │    read_until = 1709142000
  │    content = NIP-44 encrypted to own root pubkey
  │
  └─► Relay stores (parameterized replaceable, keyed by d tag)

Desktop comes online
  │
  ├─ Fetches kind 10052 where d = Bob's pubkey
  ├─ Decrypts content → read_until = 1709142000
  ├─ Marks Bob's messages up to that timestamp as read
  │
  └─ If desktop reads further (to timestamp 1709143000):
       └─ Publishes new kind 10052 with read_until = 1709143000
            (replaces mobile's version — later read_until always wins)
```

## Storage Pact Formation

```
User needs storage partners
  │
  ├─ Publishes kind 10055 (storage pact request):
  │    volume = 50MB, min_pacts = 5
  │
  ├─ WoT peers with similar volume see the request
  │   └─ Peer responds with kind 10056 offer
  │
  ├─ User selects partner
  │   ├─ Both exchange kind 10053 (private, via encrypted DM)
  │   └─ Both begin storing each other's events from current checkpoint
  │
  └─ Repeat until min_pacts fulfilled
```

Client behavior adapts with pact count: bootstrap phase (0–5, relay-primary), hybrid (5–15, mixed), sovereign (15+, peer-primary). See [Storage > Three-Phase Adoption](../architecture/storage.md#three-phase-adoption-model).

## Storage Peer Retrieval

```
Bob wants Alice's events (Alice's devices are offline)
  │
  ├─ Publishes kind 10057 (data request):
  │    bp = H(Alice's pubkey || YYYY-MM-DD)
  │    since = last known checkpoint
  │
  ├─ Alice's storage peers (other users' clients) are subscribed via relay
  │   ├─ Each peer computes H(stored_pubkeys || date) and matches against bp tag
  │   └─ Matching peer responds privately with kind 10058 (data offer)
  │        relay = connection endpoint
  │
  ├─ Bob connects to responding peer
  │   └─ Fetches Alice's events since checkpoint
  │
  ├─ Bob verifies every event signature
  │   (self-authenticating — standard Nostr verification)
  │
  └─ Fallback: try relays if no storage peers respond
```

## Gossip Discovery Flow

All gossip forwarding is client-side — standard Nostr relays store and serve events, clients decide what to forward. No relay modifications required.

```
Bob wants Alice's events
  │
  ├─ Layer 0: BLE mesh (if enabled)
  │   ├─ Check if any nearby devices have Alice's events via BLE
  │   ├─ Noise Protocol encrypted session between mesh peers
  │   ├─ Multi-hop relay up to 7 hops through intermediate devices
  │   ├─ Interop with bitchat mesh network
  │   └─ If BLE responds → done
  │
  ├─ Layer 1: Check cached endpoints
  │   ├─ Bob has kind 10059 from Alice (NIP-59 gift wrapped, only Bob can decrypt)
  │   ├─ Connect directly to cached peers
  │   └─ If cached endpoints respond → done
  │
  ├─ Layer 2: Gossip (hardened — see ADR 008)
  │   ├─ Send kind 10057 to directly connected peers (TTL=3)
  │   │   └─ Include request_id tag for deduplication
  │   ├─ Each peer receiving a request:
  │   │   ├─ Check request_id against LRU cache → drop if duplicate
  │   │   ├─ Check source pubkey rate limit (50 req/s) → drop if exceeded
  │   │   ├─ Check if source is within 2-hop WoT → serve locally but don't forward if not
  │   │   ├─ Check if they can respond (have the data)
  │   │   └─ If not, decrement TTL and forward to peers
  │   │       └─ Priority: pact partners first, then WoT, then extended WoT (ADR 009)
  │   ├─ Reaches ~8,000 nodes in a 20-peer network (20^3)
  │   ├─ Responses route back along the same path
  │   └─ If gossip responds → done
  │
  └─ Layer 3: Relay fallback
      ├─ Traditional DVM broadcast via relay
      └─ Existing kind 10057/10058 flow
```

## Storage Challenge-Response

```
Alice verifies Bob holds her data (periodic, jittered)
  │
  ├─ Sends kind 10054 (storage challenge):
  │    challenge = random nonce
  │    range = events [47..53]
  │
  ├─ Bob computes H(events[47..53] || nonce) from local copy
  │   └─ Responds with hash
  │
  ├─ Alice verifies hash against her own copy
  │   ├─ Match → Bob has the data, pact continues
  │   └─ Mismatch or timeout:
  │        ├─ Retry once (network issues)
  │        ├─ Ask other storage peers for same range
  │        ├─ If others have it → Bob's problem → 48h grace
  │        └─ Grace expires → drop pact, broadcast kind 10055 for replacement
  │
  └─ Bob loses Alice's reciprocal storage (natural consequence)
```

## Completeness Verification

```
Bob fetches Alice's events from a storage peer
  │
  ├─ Fetches Alice's latest checkpoint (kind 10051)
  │    merkle_root = abc123, event_count = 100
  │
  ├─ Receives 100 events from storage peer
  │
  ├─ Computes Merkle root of received events
  │   ├─ Computed root matches checkpoint → complete set ✓
  │   └─ Mismatch or count ≠ 100 → peer is withholding events ✗
  │        └─ Try another storage peer or relay
  │
  └─ Signatures verified + Merkle root matches = authentic AND complete
```

## Per-Event Chain Verification

```
Bob receives events from Alice's device
  │
  ├─ Check sequence numbers
  │   ├─ Events: seq 44, 45, 46, 48  ← gap at 47
  │   └─ Missing event detected without checkpoint
  │
  ├─ Check prev_hash chain
  │   ├─ Event 45: prev_hash = H(event_44.id) ✓
  │   ├─ Event 46: prev_hash = H(event_45.id) ✓
  │   └─ Mismatch → tampered or reordered chain
  │
  └─ Combined with checkpoint:
       ├─ Per-event chain → single-device stream completeness
       └─ Checkpoint Merkle root → cross-device verification
```

## Social Recovery Flow

```
Setup: User designates recovery contacts
  │
  ├─ Publishes kind 10060 for each recovery contact:
  │    d = recovery_contact_root_pubkey
  │    threshold = 3, total = 5
  │    content = NIP-44 encrypted instructions
  │    signed by root key
  │
  └─ Each contact receives and stores their 10060 (encrypted to them)

Recovery: User loses root key
  │
  ├─ User generates new root keypair
  │
  ├─ Contacts N recovery contacts out-of-band (phone, in person, etc.)
  │   └─ Each contact verifies user's identity
  │
  ├─ Each cooperating contact publishes kind 10061:
  │    p = old_root_pubkey
  │    new_root = new_root_pubkey
  │    timelock_start = now
  │
  ├─ N valid attestations exist → 7-day timelock begins
  │   │
  │   ├─ During timelock:
  │   │   ├─ Old root key can cancel (proves not actually lost)
  │   │   └─ Clients show "recovery in progress" for the identity
  │   │
  │   └─ After 7 days with no cancellation:
  │       ├─ Clients accept new root key as identity successor
  │       ├─ New root key publishes kind 10050 with device delegations
  │       └─ Identity continues under new root key
```

## Cascading Read-Cache Flow

```
Bob fetches Alice's events from storage peer S1
  │
  ├─ Bob now has a local copy of Alice's recent events
  │
  ├─ Carol broadcasts kind 10057 for Alice's data
  │   └─ Bob's client sees the request (has Alice's events cached)
  │
  ├─ Bob responds with kind 10058 (data offer)
  │   └─ Carol fetches Alice's events from Bob
  │
  └─ Carol verifies Alice's signatures — source doesn't matter
      └─ Popular data naturally replicates across the follower base
```

- No pact needed — Bob serves cached data he already has
- Load scales with O(followers), not O(storage_peers)
- See [Storage > Cascading Read-Caches](../architecture/storage.md#cascading-read-caches)

## Nearby Discovery Flow

```
User activates nearby mode (inspired by bitchat — https://github.com/permissionlesstech/bitchat)
  │
  ├─ Client generates ephemeral keypair:
  │    NOT derived from root key
  │    NOT listed in kind 10050
  │    NOT linked to any persistent identity
  │
  ├─ Broadcasts presence via BLE mesh and/or local gossip:
  │    Geohash tag ["g", "<geohash>"] at user-chosen precision
  │    No root_identity tag — completely anonymous
  │
  ├─ Discovers other nearby users in nearby mode:
  │   └─ Sees ephemeral-key-signed content with geohash tags
  │
  ├─ Optional: private identity reveal
  │   ├─ Encrypted NIP-44 exchange via ephemeral keys
  │   ├─ Each side reveals their root pubkey
  │   └─ Both can follow each other via normal Gozzip identity
  │
  └─ Deactivate nearby mode → ephemeral key discarded, no trace
```

## Lightning Boost Flow

```
User wants to boost a post through a discovery relay
  │
  ├─ Sends kind 9734 (zap request) to relay's root pubkey:
  │    amount = relay's boost price
  │    e = event_id of the post to boost
  │
  ├─ LNURL server processes payment
  │   └─ Publishes kind 9735 (zap receipt)
  │
  ├─ Relay detects zap receipt for its pubkey:
  │   ├─ Verifies payment amount meets boost threshold
  │   ├─ Adds event to curated feed, marked as boosted
  │   └─ Pushes to relay subscribers
  │
  └─ Subscribers see the post in relay's curated feed
      └─ Boosted content is transparently marked
```

## Group Message Flow

```
User (device key)
  │
  ├─ Signs kind 9 event with device subkey
  ├─ Adds root_identity tag
  │
  └─► Group relay
       ├─ Validates membership against root identities (not device keys)
       └─ Delivers to group members
```
