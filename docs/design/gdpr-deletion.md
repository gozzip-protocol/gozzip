# GDPR Deletion and Content Reporting
**Date:** 2026-03-14
**Status:** Draft

## Overview

User data in Gozzip is stored across 20+ devices via reciprocal storage pacts, cached on follower devices via read-caches, persisted on relays, and potentially replicated over BLE mesh. GDPR Article 17 ("right to erasure") requires a protocol-level mechanism to request deletion across all these storage locations. Without one, EU distribution is blocked.

This document specifies the deletion request protocol, pact partner obligations, relay behavior, cache eviction, full account deletion, content reporting, and the legal analysis required for a Data Protection Impact Assessment (DPIA).

The protocol already defines Kind 10063 (Deletion Request) and Kind 10064 (Content Report) in the [Messages](../protocol/messages.md) specification. This document extends those definitions with operational semantics, compliance timelines, enforcement mechanisms, and legal framing.

## Deletion Request: Kind 10063

### Event Format

```json
{
  "kind": 10063,
  "pubkey": "<root_pubkey>",
  "created_at": 1741996800,
  "tags": [
    ["e", "<event_id_1>"],
    ["e", "<event_id_2>"],
    ["e", "<event_id_3>"],
    ["all_events", "true"],
    ["reason", "user_request"],
    ["protocol_version", "1"]
  ],
  "content": "",
  "sig": "<signed by root key or governance key>"
}
```

**Tag definitions:**

| Tag | Required | Values | Description |
|-----|----------|--------|-------------|
| `e` | One of `e` or `all_events` | Event ID (hex) | Specific event IDs to delete. Multiple `e` tags allowed. |
| `all_events` | One of `e` or `all_events` | `"true"` | Delete all events authored by this pubkey. Used for account deletion. |
| `reason` | Optional | `user_request`, `content_violation`, `account_deletion` | Informational. Deletion requests are valid regardless of reason. |
| `protocol_version` | Required | `"1"` | Protocol version tag. |

**Authorization rules:**

- MUST be signed by root key or governance key. Device keys cannot issue deletion requests -- this prevents a compromised device from wiping an identity's event history.
- The `pubkey` field must match the author of the events referenced by `e` tags. A user cannot request deletion of another user's events.
- Governance key authorization is sufficient because governance key holders are trusted (same level as profile/follow list management).

**Propagation:**

- Deletion requests propagate via the same channels as regular events: published to the author's relays (NIP-65 relay list), forwarded to pact partners via existing event delivery channels, and propagated via gossip (kind 10057/10058 flow).
- Pact partners receive deletion requests through their normal event subscription for the author.
- Read-caches encounter deletion requests when syncing from relays or pact partners.

### Selective vs. Full Deletion

**Selective deletion** (one or more `e` tags): Targets specific events. The deletion request itself is retained by relays and pact partners as a tombstone -- it prevents re-ingestion of the deleted events from other sources. Clients encountering a deletion request for an event they hold should delete the event and retain the deletion request.

**Full deletion** (`all_events` tag): Deletes all events authored by this pubkey. Used as part of the account deletion flow (see below). The deletion request itself is the final event retained as a tombstone.

## Pact Partner Obligations

### Deletion Timeline

Upon receiving a valid kind 10063 deletion request, pact partners MUST delete the specified events within **72 hours**.

The 72-hour window accounts for:
- Intermittent devices (Witnesses at 30% uptime) that may not process the request immediately
- BLE mesh devices that sync infrequently
- Batch processing on resource-constrained devices

### Verification via Challenge-Response

Deletion compliance can be verified using the existing challenge-response mechanism (kind 10054):

1. After the 72-hour window, the requesting client issues a `hash` challenge covering a range that includes the deleted event's sequence position.
2. If the pact partner includes the deleted event in the hash computation, the hash will not match the expected value (computed without the deleted event).
3. If the pact partner has deleted the event, the hash matches.

This is not a cryptographic proof of deletion -- a pact partner could maintain a shadow copy while returning the correct hash. But it verifies protocol compliance at the data-serving layer.

**Challenge for a specific deleted event:**

The requesting client can also issue a `serve` challenge (for Full-Full pairs) requesting the specific deleted event by sequence number. If the partner serves it, deletion has not occurred. If the partner returns nothing or an error, deletion is confirmed at the serving layer.

### Enforcement via Pact Degradation

Failure to delete after 72 hours triggers the same consequences as a failed challenge:

1. **First violation:** Reliability score decreases. Challenge frequency increases to 3x normal (same as Degraded state).
2. **Repeated violations:** Score drops below threshold. Pact transitions to Failed, then Dropped. Standard pact state machine applies (see [Pact State Machine](../protocol/pact-state-machine.md)).
3. **Replacement:** Client begins seeking a replacement pact partner via kind 10055.

This is the same consequence as failing storage challenges. The pact incentive structure -- where reliable storage is rewarded with gossip priority and unreliable storage leads to pact loss -- extends naturally to deletion compliance.

### Honest Limitation

Honest deletion cannot be cryptographically verified in a decentralized system. A pact partner can:
- Delete the event from their serving layer while retaining a copy elsewhere
- Claim deletion while keeping data on a separate disk
- Pass challenge-response verification while maintaining shadow copies

The protocol relies on the same incentive that makes storage pacts work: pact partners who behave dishonestly (whether by not storing or by not deleting) risk pact degradation and loss of the reciprocal storage they depend on. This is a "reasonable efforts" approach, not a guarantee.

## Relay Obligations

### NIP-09 Compatibility

Standard Nostr relays already support NIP-09 deletion events (kind 5). Kind 10063 extends this with Gozzip-specific semantics:

- Relays SHOULD treat kind 10063 as equivalent to NIP-09 kind 5 for retention purposes
- Relays receiving a kind 10063 MUST delete the referenced events from their storage
- Relays MUST NOT serve deleted events in response to future queries
- Relays SHOULD retain the kind 10063 event itself as a tombstone to prevent re-ingestion

### Root Key Authorization

Unlike NIP-09 (where any key that signed an event can delete it), kind 10063 requires root key or governance key authorization. This means:

- A device key that originally signed a kind 1 note cannot issue a kind 10063 to delete it
- Only the root identity owner (via root key or governance key) can request deletion
- This prevents a compromised device from wiping history while limiting the blast radius of device compromise

Relays that index by `root_identity` tag can verify authorization by checking the kind 10063 signer against the kind 10050 delegation chain.

### Relay Compliance Verification

Relay compliance is verifiable by re-querying:

1. Client publishes kind 10063 to relay
2. After processing time (seconds to minutes), client queries for the deleted event IDs
3. If the relay returns the events, the relay has not honored the deletion request
4. Client can drop the non-compliant relay from their NIP-65 relay list

This is stronger verification than pact partner deletion because relays are queried directly -- there is no hash-based indirection.

## Read Cache Expiration

### Tiered Cache TTLs

All read caches already honor maximum TTLs based on feed tier (see [Storage](../architecture/storage.md#tiered-cache-ttls)):

| Feed Tier | Default TTL |
|-----------|------------|
| Inner Circle | 30 days (pact-managed) |
| Orbit | 14 days |
| Horizon | 3 days |
| Relay-only | 1 day |

Without an explicit deletion request, cached content expires naturally within these windows.

### Forced Cache Eviction

Upon receiving a kind 10063 deletion request, read-caches MUST evict the specified events immediately -- regardless of remaining TTL.

**Implementation:**

1. Client receives kind 10063 through normal event subscription (relay sync, pact partner delivery, or gossip)
2. Client checks local read-cache for any matching event IDs (from `e` tags) or all events by the author (if `all_events` is set)
3. Matching cached events are deleted immediately
4. The kind 10063 event is retained locally to prevent re-caching from other sources

**Gossip propagation ensures reach:** Because deletion requests propagate via the same gossip channels as regular events (2-hop WoT boundary, TTL=3), they reach the same devices that might hold cached copies. A deletion request for a popular author's content will propagate through the same follower base that cached the content in the first place.

### BLE Mesh Copies

Events propagated via BLE mesh (BitChat integration) have a natural expiry based on the mesh relay hop limit (7 devices max) and device-local storage constraints. BLE copies cannot be targeted by kind 10063 because BLE mesh devices may be offline indefinitely. These copies persist until:

- Natural storage expiry on the device
- Device storage pressure triggers LRU eviction
- The device comes online and processes the deletion request via standard channels

This is an acknowledged gap -- see Limitations below.

## Account Deletion (Full Erasure)

### Trigger

A user who wants to fully delete their Gozzip identity issues a kind 10063 with the `all_events` tag and `reason: account_deletion`.

### Sequence of Operations

Account deletion is the root key's final act. The following operations execute in order:

**Step 1: Publish deletion request**

```json
{
  "kind": 10063,
  "pubkey": "<root_pubkey>",
  "created_at": 1741996800,
  "tags": [
    ["all_events", "true"],
    ["reason", "account_deletion"],
    ["protocol_version", "1"]
  ],
  "content": "",
  "sig": "<signed by root key>"
}
```

**Step 2: Dissolve all pacts**

The client publishes kind 10053 with `status: dropped` to each pact partner (encrypted). Partners receive the drop notification through their normal event channels.

**Step 3: Revoke all device keys**

The client publishes a final kind 10050 with an empty device list -- no authorized devices, no DM key, no governance key. This effectively revokes all delegation. Any future events signed by the former device keys will fail authorization checks against this empty kind 10050.

**Step 4: Delete local data**

The client deletes all local event data, cached data, keys, and configuration from all devices the user controls.

### Partner Obligations

| Participant | Obligation | Timeline |
|------------|-----------|----------|
| Relays | Delete all events by this pubkey. Retain the kind 10063 tombstone. | Immediate |
| Active pact partners | Delete all stored events for this user. The pact is already dissolved (Step 2). | 72 hours |
| Standby pact partners | Same as active. | 72 hours |
| Read-caches (followers) | Evict all cached events by this pubkey. | Immediate on receipt |
| Former pact partners (already dropped) | No obligation -- they already deleted stored events when the pact was dropped (7-day grace period from standard pact drop). | N/A |

**30-day full purge window:** Pact partners have 30 days to fully purge all stored data associated with the deleted account, including metadata, challenge logs, and reliability scores. The 72-hour window covers event data; the 30-day window covers auxiliary data that may reference the deleted user.

## Legal Analysis

### GDPR Roles

**Controller vs. Processor:** In GDPR terms, each participant that stores user data is an independent data controller, not a processor acting on the user's behalf. This applies to:

- **Pact partners:** Each pact partner independently decides to store the user's data (via mutual pact agreement). They are controllers of the data they hold.
- **Relay operators:** Relay operators set their own retention policies and decide what to store. They are controllers.
- **Read-cache holders:** Followers who cache content make an independent decision to cache. They are controllers of their cached copies.

This controller-controller model is important because it means:
- The original author is not responsible for ensuring every controller deletes data -- they are responsible for providing a deletion mechanism and making reasonable efforts.
- Each controller is independently responsible for honoring deletion requests under their own GDPR obligations.
- No single party is the "data processor" acting under contract -- all parties are independent controllers with bilateral agreements (pacts).

### Lawful Basis for Storage

The lawful basis for pact partners storing user data is **legitimate interest** (Article 6(1)(f)):

- The bilateral pact agreement represents mutual legitimate interest -- both parties benefit from reciprocal storage.
- The data subject (user) initiates the pact through their client's pact formation behavior.
- The user can withdraw at any time by dropping the pact or issuing a deletion request.

For relay storage, the lawful basis is the same: users publish events to relays voluntarily, and relays store them under their stated retention policies.

### Right to Erasure (Article 17)

The protocol satisfies Article 17 requirements through:

1. **Mechanism exists:** Kind 10063 provides a protocol-level deletion request that reaches all data holders.
2. **Reasonable efforts:** The protocol propagates deletion requests via gossip to all reachable nodes. Pact partners are incentivized to comply through pact degradation. Relays are verifiably compliant via re-query.
3. **Timeline:** 72-hour deletion window for pact partners. Immediate for relays. Natural expiry for caches.
4. **Account deletion:** Full erasure path via the account deletion flow.

### Right to Portability (Article 20)

Users already satisfy data portability requirements:

- Users control their root key and all derived keys.
- Users can export all their events (self-authenticating signed data) at any time.
- Events are stored in a standard format (Nostr events) readable by any compatible client.
- No vendor lock-in -- switching clients requires only the root key.

### Data Protection Impact Assessment (DPIA)

A DPIA (Article 35) is required before EU launch because:

- The protocol involves large-scale processing of personal data (user-generated content stored across distributed nodes).
- Data is stored on devices outside the user's direct control (pact partner devices).
- Cross-border data transfers are inherent (pact partners may be in any jurisdiction).

The DPIA should address:
- Data flows between pact partners, relays, and caches
- Encryption measures (NIP-44 for DMs, NIP-59 for gift wrapping)
- Deletion mechanism effectiveness and limitations
- Cross-border transfer safeguards (pact partners in non-GDPR jurisdictions)
- Risk to data subjects if a pact partner refuses to delete

### Reasonable Efforts Standard

GDPR recognizes that deletion in distributed systems is best-effort. Recital 66 states that the controller should take "reasonable steps, including technical measures" to inform other controllers of the erasure request. The protocol satisfies this:

- **Technical measure:** Kind 10063 deletion request, propagated via gossip.
- **Informing other controllers:** Deletion request reaches pact partners (bilateral controllers) and relays (independent controllers) through normal event delivery.
- **Reasonable steps:** Pact degradation incentivizes compliance. Relay compliance is verifiable. Cache TTLs provide natural expiry.

What the protocol cannot guarantee -- and what GDPR does not require:
- Deletion on non-compliant nodes outside the author's reach
- Deletion of copies made by humans (screenshots, manual exports)
- Deletion on devices that are permanently offline

## Limitations and Honest Acknowledgments

### Cannot Force Deletion on Non-Compliant Nodes

A decentralized network cannot force any node to delete data. The protocol provides the mechanism (kind 10063) and the incentive (pact degradation), but a node that ignores deletion requests and accepts the pact consequences cannot be compelled by the protocol.

**Mitigation:** Non-compliant nodes lose their pacts and the reciprocal storage they depend on. This is a natural consequence, not a punishment -- but it does not guarantee deletion.

### BLE Mesh Copies May Persist

Events replicated via BLE mesh to devices that are offline or outside internet connectivity will persist until:
- The device comes online and processes the deletion request
- Natural storage expiry (LRU eviction under storage pressure)
- The device owner manually clears data

BLE mesh has a 7-hop limit and content is ephemeral by nature (small messages, limited storage on BLE devices). The risk surface is small but non-zero.

### Screenshots and Manual Copies Are Outside Protocol Scope

Once an event is displayed on a screen, a human can screenshot, copy-paste, or manually transcribe the content. No protocol can prevent this. This is the same limitation faced by every messaging and social media platform.

### Relay Operators in Non-GDPR Jurisdictions

Relay operators outside the EU are not bound by GDPR. A relay in a non-GDPR jurisdiction may choose to honor kind 10063 deletion requests (many will, for interoperability), but the protocol cannot enforce this.

**Mitigation:** Users can choose relays in GDPR jurisdictions. The NIP-65 relay list gives users control over which relays store their data. Clients can surface relay jurisdiction information to help users make informed choices.

### Comparison to Existing Systems

| System | Architecture | Deletion capability | Guarantee level |
|--------|-------------|-------------------|----------------|
| Signal | Centralized | Full server-side deletion; disappearing messages | Strong (single controller) |
| Matrix | Federated | Redaction events; homeservers should honor them | Medium (federation-dependent) |
| Nostr | Relay-based | NIP-09 kind 5 deletion; relay-dependent | Weak (relay discretion) |
| Gozzip | Distributed pacts | Kind 10063 + pact degradation + cache eviction | Medium (incentive-enforced) |

Gozzip's deletion guarantee is stronger than standard Nostr (pact degradation adds economic incentive beyond relay goodwill) but weaker than centralized systems (no single point of enforcement). This is an inherent trade-off of decentralization. The protocol is honest about this trade-off rather than claiming guarantees it cannot provide.

## Content Reporting: Kind 10064

### Event Format

```json
{
  "kind": 10064,
  "pubkey": "<reporter_root_pubkey>",
  "created_at": 1741996800,
  "tags": [
    ["e", "<reported_event_id>"],
    ["p", "<reported_author_pubkey>"],
    ["report", "harassment"],
    ["protocol_version", "1"]
  ],
  "content": "Repeated targeted harassment in replies. Screenshots available on request.",
  "sig": "<signed by device key>"
}
```

**Report categories** (`report` tag):

| Category | Description | Expected action |
|----------|-------------|----------------|
| `spam` | Unsolicited commercial or repetitive content | Relay filtering; mute suggestion |
| `harassment` | Targeted abuse, threats, doxxing | Relay removal; potential law enforcement |
| `illegal` | Content illegal in reporter's jurisdiction | Relay removal; jurisdiction-dependent |
| `csam` | Child sexual abuse material | Immediate deletion; law enforcement referral |
| `impersonation` | Fake identity claiming to be another person | Relay labeling; community awareness |
| `other` | Does not fit predefined categories | Free-text description in `content` field |

### Report Routing

Reports are published to:

1. **Reporter's relays** -- standard event publication via NIP-65 relay list.
2. **Reported author's relays** -- published to relays listed in the reported author's NIP-65 relay list, so relay operators who serve the reported content can act.
3. **Community moderators** (optional) -- if the reporter is in a NIP-29 group with designated moderators, the report can be addressed to moderator pubkeys.
4. **External moderation services** (optional) -- clients may forward reports to third-party content moderation services that monitor kind 10064 events. This is client behavior, not protocol mandated.

### Handling Illegal Content (CSAM)

Pact partners storing potentially illegal content face legal exposure. The protocol provides an explicit escape valve as defined in the [Moderation Framework](moderation-framework.md):

1. If a pact partner receives a credible report of illegal content (kind 10064 with `report: csam` or `report: illegal`), they MAY:
   - Delete the specific content immediately
   - Drop the pact with no reliability penalty
2. The dropped pact does not count against the partner's reliability score.
3. Clients SHOULD treat pact drops accompanied by a `csam`/`illegal` report differently from unexplained pact drops.

"Credible report" is deliberately undefined at the protocol level -- this is a judgment call for the pact partner's operator or user.

### Emergency Takedown Path

For content requiring immediate removal (CSAM, imminent threats):

1. **Relay operators** are the emergency takedown authority. They can delete any event from their relay at any time -- this is standard relay operator behavior in Nostr.
2. **Pact partners** can invoke the illegal content escape valve to delete content and drop pacts immediately.
3. **No protocol-level emergency authority exists.** There is no admin, no central moderator, no kill switch. This is by design -- the protocol provides tools, not policy.

The emergency path is relay-operator-first because relay operators are the most accessible infrastructure operators. Pact partner deletion follows via the escape valve. Content that persists on non-responsive pact partners or offline devices will be caught by the 72-hour deletion window if a kind 10063 is also issued.

### Client-Side Content Filtering (NIP-32)

Third-party labeling services (NIP-32 compatible) provide client-side content filtering:

1. Labeling services publish kind 1985 label events tagging content.
2. Clients subscribe to labeling services they trust (WoT-based selection).
3. Labels are advisory -- clients decide how to use them (hide, warn, show).
4. Multiple labeling services can label the same content differently.
5. Labels from pubkeys within the user's 2-hop WoT are weighted higher.

No new event kinds are needed for this -- NIP-32 labeling works with existing Nostr infrastructure.

## Implementation Checklist

For protocol implementers building GDPR-compliant Gozzip clients:

- [ ] Publish kind 10063 on user-initiated deletion (selective and full)
- [ ] Process incoming kind 10063: delete referenced events, retain tombstone
- [ ] Evict cached content immediately on kind 10063 receipt
- [ ] Verify pact partner deletion via challenge-response after 72h
- [ ] Degrade non-compliant pact partners (same as failed challenge)
- [ ] Implement account deletion flow (kind 10063 + pact dissolution + key revocation)
- [ ] Verify relay deletion via re-query
- [ ] Support kind 10064 content reports (publish and receive)
- [ ] Honor `csam`/`illegal` pact drop without reliability penalty
- [ ] Surface relay jurisdiction information to users (NIP-65 relay selection)
- [ ] Implement 30-day auxiliary data purge for account deletions
- [ ] Retain deletion request tombstones to prevent re-ingestion

## Open Questions

1. **Tombstone retention period.** How long should kind 10063 tombstones be retained? Indefinitely prevents re-ingestion but consumes storage. A 1-year retention with periodic re-publication by the author's client is a possible compromise.

2. **Deletion request for delegated content.** If a user's event is reposted (kind 6) by another user, does the original author's deletion request cover the repost? Current answer: no -- the repost is a separate event signed by a different author. The original author can delete their original event; the reposter must delete the repost.

3. **Pact partner deletion audit log.** Should clients maintain a local audit log of deletion requests sent and compliance verified? This would support DPIA documentation but adds local storage overhead.
