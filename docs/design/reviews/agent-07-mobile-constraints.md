# Review: Mobile Platform Constraints

**Reviewer:** Agent 07 — Mobile Platform Engineer (iOS/Android shipping experience)
**Date:** 2026-03-12
**Scope:** Whether the Gozzip protocol is realistic on mobile phones given battery, bandwidth, storage, background execution, and App Store constraints.
**Verdict:** The protocol has critical mobile gaps (push notifications, UX parity with existing messaging), but the phased rollout (Phase 1 targets desktop/extension, mobile is Phase 2) buys time. The core architecture is sound for mobile -- storage and bandwidth requirements are modest, and multi-device sync works well. Key management UX and App Store compliance require careful engineering.

---

## 1. Storage on Mobile

### Protocol assumption

The whitepaper claims "<1 GB (power user)" storage with "100x+ margin" on a 128-256 GB phone. The protocol requires storing events for ~20 pact partners, each within a 30-day window.

### Reality of mobile platform constraints

The storage math from the whitepaper: E[size] = 925 bytes per event, 25 events/day per user. Per pact partner (Light node, 30-day window):

```
925 bytes * 25 events/day * 30 days = 694 KB per partner
```

For 20 partners: ~13.5 MB for pact obligations. With read cache (default 100 MB): ~114 MB total. This is genuinely modest.

However, the calculation uses the "default content mix" which allocates only 5% of events to kind 30023 (long-form, 5500 bytes). Real social networks are increasingly media-heavy. The protocol does not store media blobs (images, video) in events -- those are external URLs -- so the event sizes are realistic for text-based social content.

**The real storage concern is not total size but write amplification and SQLite overhead.** Storing 20 partners' event streams means maintaining indexes, hash chains, Merkle trees, and sequence tracking. A SQLite database with proper indexes for 20 partners' 30-day windows, plus the read cache, plus local event history, will realistically occupy 200-500 MB. This is well within mobile storage capacity.

**iOS storage concerns:** iOS can reclaim app storage under pressure. Apps should handle the `NSFileProtectionComplete` attribute and be prepared for storage pressure callbacks. The 200-500 MB footprint is acceptable -- apps like Spotify, Netflix, and Telegram routinely use 1-10 GB.

### Gap assessment

**Severity: Minor.**

Storage is the one area where the protocol's claims are approximately correct. The "<1 GB" claim holds for text-based social content. The "100x+ margin" is an exaggeration (a 64 GB iPhone with 20 GB free has 40-100x margin for 200-500 MB, but 64 GB phones with heavy usage may have only 5-10 GB free). The claim is directionally correct, just overstated.

### Suggested fix

1. State realistic total footprint: 200-500 MB including indexes and caches, not just raw event bytes.
2. Make the read cache configurable with an aggressive default for phones (50 MB instead of 100 MB).
3. Implement storage pressure handling: shed read cache first, then reduce pact partner window from 30 to 14 days under pressure.

---

## 2. Bandwidth on Metered Connections

### Protocol assumption

The whitepaper claims "4.1 MB/day (Light)" with "100x+ margin" against 1-5 GB WiFi. The protocol's bandwidth comes from: event sync with pact partners, challenge-response messages, gossip forwarding, checkpoint reconciliation, and pact negotiation/maintenance.

### Reality of mobile platform constraints

The 4.1 MB/day calculation needs decomposition:

**Event sync (receiving):** 20 partners * 25 events/day * 925 bytes = 463 KB/day. With protocol overhead (WebSocket framing, TLS, Nostr envelope): approximately 650 KB/day.

**Event sync (sending own events):** 25 events/day * 925 bytes * 20 partners = 463 KB/day. But the protocol uses relay-mediated delivery for intermittent devices, so the phone sends to the relay once and partners fetch. Outbound: ~25 KB/day. Partners pull from relay independently.

**Challenge-response:** 20 partners, assume 1 challenge per partner per day (24h window). Each challenge is ~200 bytes, each response is ~200 bytes. Total: ~8 KB/day. Negligible.

**Gossip forwarding (when in foreground):** This is the wildcard. If the phone forwards gossip requests (kind 10057) for WoT peers, each request is ~200 bytes, but the forwarding fan-out amplifies this. With 20 peers and TTL=3, a single gossip request that the phone participates in could involve receiving 20 requests and forwarding up to 20 copies. During active foreground use, a moderately connected user might handle 50-200 gossip requests per session. At ~400 bytes per request/forward cycle: 20-80 KB per active session. If the user has 3-4 sessions per day: 60-320 KB/day from gossip.

**Checkpoint reconciliation:** One checkpoint per sync session, containing device heads and Merkle root. Each checkpoint is ~500 bytes. Fetching missed events after being offline: variable, but bounded by the 30-day window. Coming online after 8 hours offline with 20 active partners: 20 * 25/3 events * 925 bytes = ~154 KB per sync. With 3-4 syncs per day: 460-615 KB/day.

**Total realistic estimate:** 1.5-3 MB/day for a moderately active Light node. The 4.1 MB/day figure is in the right ballpark.

**Monthly total on mobile data:** 45-90 MB/month. This is viable even on a 2 GB/month plan (2-5% of quota).

**The real bandwidth concern is burst behavior.** After being offline for 24 hours, reconnecting and syncing 20 partners' events requires downloading ~460 KB in a burst. After being offline for a week (vacation, no WiFi), the burst is ~3.2 MB. These are manageable.

### Gap assessment

**Severity: Minor.**

The bandwidth claims are approximately correct for steady-state operation. The "100x+ margin" against WiFi is accurate but irrelevant -- the question is whether it works on metered cellular, and it does. 45-90 MB/month is viable in developing countries. The protocol is actually well-designed for bandwidth-constrained environments.

The one concern: if a user follows 150 people (Dunbar number) and the read cache aggressively fetches content for all of them, bandwidth could spike. But the tiered cache TTL design (Inner Circle 30d, Orbit 14d, Horizon 3d, Relay-only 1d) suggests fetch-on-demand rather than background pre-fetch, which is bandwidth-appropriate.

### Suggested fix

1. Add a "data saver" mode that disables gossip forwarding and reduces read cache TTLs.
2. Allow users to set a monthly data budget; the client throttles sync when approaching the limit.
3. Prefer WiFi for bulk sync (checkpoint reconciliation after long offline periods). Queue non-urgent sync for WiFi.

---

## 3. Key Management on Mobile

### Protocol assumption

Root key is in cold storage (hardware wallet, air-gapped device). Device subkeys sign day-to-day events. DM key rotates every 90 days via KDF. Governance key signs profile/follow list updates. Social recovery with N-of-M threshold using kind 10060/10061.

### Reality of mobile platform constraints

**Root key in cold storage:** This is good security but terrible UX. The protocol requires the root key for: publishing kind 10050 (device delegation), designating recovery contacts (kind 10060), and revoking compromised devices. How often does an average user need to do these things?

- Adding a new device: requires root key. User buys a new phone -- they need their hardware wallet to authorize it. If they don't have the hardware wallet at the phone store, they cannot set up Gozzip on their new phone.
- Revoking a lost device: requires root key. Phone gets stolen -- user needs cold storage access to revoke the stolen device's subkey. If the root key is a hardware wallet in a safe at home and the user is traveling, they cannot revoke the compromised device until they get home.
- DM key rotation every 90 days: the DM key is derived via `KDF(root, "dm-decryption-" || rotation_epoch)`. If this derivation requires the root private key, the user needs their hardware wallet every 90 days. If the derivation can be done with just the root *public* key and a stored seed, this is fine -- but the protocol document does not clarify this.

**Device subkey storage on mobile:** The device private key must be stored securely on the phone. iOS Secure Enclave and Android Keystore provide hardware-backed key storage with biometric access. secp256k1 is not natively supported by iOS Secure Enclave (which uses P-256/NIST curves) or Android Keystore (which supports P-256, RSA). This means:

- The device key must be stored in software (encrypted by a Secure Enclave/Keystore-protected key), not directly in hardware
- Signing operations require loading the key into memory, creating a brief exposure window
- This is the same approach used by existing Nostr mobile apps, so it is a known and accepted trade-off

**Average user key management:** The protocol envisions a key hierarchy with root key, governance key, device subkeys, DM key, and checkpoint delegate authorization. An average user cannot understand or manage this complexity. The UX must hide all of it:

- "Set up your account" (generates root key, stores it... where? A paper backup? A hardware wallet the user doesn't own?)
- "Add this device" (needs root key)
- "Your messages are being secured" (DM key rotation, invisible)

The protocol's security model is excellent for technically sophisticated users. For the mass market, the root key cold storage requirement is a significant adoption barrier.

### Gap assessment

**Severity: Significant.**

The key hierarchy is sound cryptography but assumes users either own hardware wallets or understand cold storage. The mass market does not. The social recovery mechanism (kind 10060/10061) provides a safety net, but it has a 7-day timelock -- if a user loses their phone AND their root key backup, they have zero access for 7 days. For a messaging app, 7 days of inaccessibility is devastating.

The secp256k1 vs hardware enclave mismatch is a known industry problem (Nostr faces it too) and is not specific to Gozzip.

### Suggested fix

1. Define a "standard" and "advanced" key management mode. Standard: root key is encrypted and backed up to iCloud Keychain / Google Cloud Keystore, protected by the user's device passcode. Advanced: hardware wallet cold storage. The protocol already supports this; it just needs explicit UX guidance.
2. Clarify whether DM key rotation requires the root private key or can be performed by a device subkey with appropriate delegation. If it requires root, reduce rotation frequency or allow device-level delegation of key derivation.
3. For device onboarding: allow an existing authorized device to authorize a new device without the root key (device-to-device delegation), with an optional root key confirmation within 7 days. This lets users set up new phones immediately.
4. Reduce the social recovery timelock to 48-72 hours for messaging-focused deployments. 7 days is appropriate for high-value identity, not for social messaging.

---

## 4. Multi-Device Sync on Mobile

### Protocol assumption

Checkpoint reconciliation when a device comes online. Fetch kind 10050 (device list), fetch kind 10051 (checkpoint with device heads), query relay for events since last known heads, publish updated checkpoint. "The user must never notice."

### Reality of mobile platform constraints

**Sync on wake performance.** When a user opens the app after 8 hours offline, the sync sequence is:

1. Establish WebSocket to relay: 200-500ms (TCP + TLS + WebSocket upgrade)
2. Fetch kind 10050: 100-300ms (single event fetch)
3. Fetch kind 10051: 100-300ms (single event fetch)
4. Compute delta: <10ms
5. Fetch missed events from siblings: 200-1000ms (depending on how many events, relay response time)
6. Process and store: 50-200ms

Total: 0.7-2.3 seconds. This is acceptable. The user sees content within 1-2 seconds of opening the app. Comparable to Twitter/Mastodon cold-start.

**30-second session problem.** If the user opens the app for 30 seconds:
- Sync completes in ~2 seconds: good
- They read content, maybe react or post
- They background the app
- The app has ~30 seconds to: publish any events to relay, receive any gossip updates, respond to any queued challenges
- BGAppRefreshTask fires 0-1 times before next foreground

This is actually fine for basic usage. The 30-second session is enough to sync, read, and post. Gossip forwarding and challenge-response happen opportunistically -- if they happen during the session, great; if not, they happen next session. The 24h challenge window for intermittent devices accommodates this.

**Long-offline sync (days/weeks).** A user returns from a week-long vacation with no connectivity:
- 20 partners * 25 events/day * 7 days * 925 bytes = 3.2 MB of events to fetch
- Plus their own devices' events from other devices
- Plus checkpoint verification (Merkle root computation over potentially thousands of events)

This takes 5-15 seconds on a good connection. It is noticeable but not catastrophic. The user sees a brief "syncing..." state, then content appears.

**The real problem is checkpoint publication.** The protocol says any checkpoint delegate publishes an updated checkpoint after reconciling sibling events. On mobile, this means the phone publishes a kind 10051 every time it comes online and discovers new sibling events. This is a write operation that goes through the relay. If the user has a desktop that is always on (a Full node), the desktop handles checkpoint duty and the phone never needs to publish checkpoints. But if the user has only mobile devices, checkpoint publication becomes a foreground-session obligation.

### Gap assessment

**Severity: Minor.**

Multi-device sync is well-designed for mobile. The checkpoint reconciliation model is efficient (single event fetch for each step), and the 24h challenge window accommodates intermittent devices. The 30-second session scenario works. The long-offline scenario is acceptable.

The one concern is users with only mobile devices (no desktop Full node). These users' checkpoints are only updated when they open the app, meaning pact partners' view of their data completeness is always stale. This is a real but minor issue -- pact partners can still serve the events they have, even without an up-to-date checkpoint.

### Suggested fix

1. Allow checkpoints to be published by relay-side delegates (a pact partner's Full node publishes checkpoints on the mobile user's behalf). This requires checkpoint delegation to work for non-device entities, which kind 10050's `checkpoint_delegate` already supports -- just extend it to pact partner pubkeys.
2. Batch checkpoint publication with event publication in the foreground session to minimize round-trips.

---

## 5. Push Notifications

### Protocol assumption

The protocol does not explicitly address push notifications. The architecture assumes devices either maintain connections (WebSocket to relays) or sync on wake (`REQ since:`). There is no mention of APNs (Apple Push Notification Service) or FCM (Firebase Cloud Messaging).

### Reality of mobile platform constraints

**Without push notifications, the user has no way to know a new message has arrived.** They must open the app to check. This is the single most important UX gap versus Signal, WhatsApp, and Telegram. Users will not adopt a messaging app that requires manual polling.

**Push notification options:**

Option A: Centralized push service. A server monitors relays for events addressed to the user and sends push notifications via APNs/FCM. This is what Signal does (Signal's server sends push payloads, the app decrypts locally). This works but introduces a centralized component that knows when the user receives messages (though not the content, which is encrypted).

Option B: Relay-side push. Nostr relays send push notifications when events match a subscription. The relay knows the user's push token and subscription filters. This is equivalent to Option A but distributed across relays. Each relay independently sends pushes. The user's push token is exposed to relay operators.

Option C: Self-hosted push. The user runs their own notification relay (a Full node pact partner, a VPS, a Raspberry Pi) that monitors for their events and pushes via APNs/FCM. This preserves privacy but requires technical setup.

Option D: No push, just background fetch. Rely on BGAppRefreshTask/WorkManager to periodically check for new content and post local notifications. Delay: 15 minutes to hours. Unreliable. Not viable for time-sensitive messaging.

**The decentralization trade-off:** APNs and FCM are centralized services operated by Apple and Google. Every push notification on iOS goes through Apple's servers; every push on Android goes through Google's servers. This is an inescapable platform constraint. Even Signal, the gold standard of private messaging, uses APNs/FCM for notification delivery (with encrypted payloads that Apple/Google cannot read).

Using APNs/FCM does not "undermine" decentralization claims because the notification is a wake-up signal, not a data channel. The content is fetched from relays/pact partners after the app wakes. Apple/Google learn that "something happened" but not what. This is the same privacy model as every other messaging app on the platform.

### Gap assessment

**Severity: Critical.**

The absence of push notification design is the single largest mobile UX gap. A messaging app without push notifications is unusable. This is not a nice-to-have; it is a hard prerequisite for mobile viability.

### Suggested fix

1. Design a notification relay service that users can self-host or use from a trusted provider. The service holds the user's APNs/FCM token and a relay subscription filter. When matching events arrive, it sends a push with minimal metadata (just "new message" or "new activity").
2. Support multiple notification providers to avoid single-point dependency. A user can register with 2-3 notification relays.
3. For maximum privacy: the notification relay only sends a "wake up" push with no content. The app wakes, syncs from relays/pact partners, and generates the local notification with actual content.
4. For DMs specifically: the sender's client can trigger a push notification via the recipient's notification relay by publishing a wake-up event (a new kind or an ephemeral event type) that the notification relay watches for.
5. Acknowledge in the protocol docs that push notifications require interacting with Apple/Google infrastructure. This is a platform reality, not a protocol failure. Frame it correctly.

---

## 6. App Store Restrictions

### Protocol assumption

The protocol does not address App Store compliance. The whitepaper mentions Lightning (zaps, relay services) and cryptocurrency-adjacent concepts (secp256k1 keys, proof of storage).

### Reality of mobile platform constraints

**iOS App Store guidelines (relevant sections):**

- **3.1.1 In-App Purchase:** Any digital content or service purchased within the app must use Apple's In-App Purchase. Lightning payments for relay services (boost, curation) would need to go through IAP or be carefully structured as "tipping" (which Apple now permits for person-to-person tips without IAP).
- **2.3.1 Background execution:** Apps that drain battery through excessive background activity face rejection. The protocol's background sync must be demonstrably lightweight. Apple reviews battery impact.
- **4.2.6 Cryptocurrency:** Apps may facilitate approved virtual currency transmission, but must comply with state/local laws. Pure messaging/social apps that happen to use cryptographic keys are fine. Apps that enable financial transactions need compliance reviews.
- **2.5.4 Multitasking:** Apps using background modes (BLE, fetch, processing) must actually use them for their stated purpose. Using BLE background mode for general networking when no BLE functionality is active will be rejected.

**Practical App Store risks:**

1. **BLE background mode abuse:** If the app registers `bluetooth-central` or `bluetooth-peripheral` background modes, Apple reviewers will verify that BLE is genuinely used. Using BLE background mode solely to keep the app alive for gossip processing would be rejected. The protocol's genuine BLE mesh transport justifies this background mode, but only when BLE is actively enabled by the user.

2. **Lightning/Zaps integration:** NIP-57 zaps involve Lightning Network payments. Apple's position on cryptocurrency payments in apps has been contentious (Damus was briefly threatened). Current guidance (post-2024) allows cryptocurrency payments for digital tips but not for unlocking app features. Relay service payments (boost, curation) may fall in a gray area.

3. **Battery usage:** If the app consistently appears in users' battery usage screens at >10%, Apple may flag it in review or users will leave 1-star reviews. The protocol must be battery-conservative by default.

**Google Play risks:** Generally more permissive than Apple. Cryptocurrency apps are allowed. Background execution is more flexible. The main risk is battery optimization -- Android's "restrict background activity" feature will throttle the app if it is detected as a high battery consumer.

### Gap assessment

**Severity: Significant.**

App Store restrictions are navigable but require careful engineering:
- BLE background mode: justified by genuine BLE mesh, but must be disabled when not in use
- Lightning payments: legal gray area on iOS, requires monitoring Apple's evolving policy
- Battery: must be conservative by default to avoid user complaints and review flags

The protocol itself is not blocked by App Store rules, but the implementation must be thoughtful.

### Suggested fix

1. Lightning/zap features should be implemented via external links (opening a Lightning wallet app) rather than in-app payment processing, sidestepping IAP requirements.
2. BLE background mode should only be registered when the user has explicitly enabled nearby/mesh features.
3. Ship with a battery-conservative default configuration. Aggressive features (BLE mesh, gossip forwarding, frequent sync) should be opt-in.
4. Do not mention "cryptocurrency," "blockchain," or "mining" in App Store metadata. Describe the app as a "decentralized social messaging app with end-to-end encryption." The cryptographic key management is an implementation detail, not a marketing feature.

---

## 7. Comparison to Existing Mobile Messaging

### Protocol assumption

The whitepaper compares Gozzip to Nostr and Mastodon on architecture but does not benchmark against the mobile messaging experience of Signal, WhatsApp, or Telegram.

### Reality of mobile platform constraints

| Feature | Signal / WhatsApp / Telegram | Gozzip (mobile) |
|---------|------------------------------|-----------------|
| Message delivery latency | < 1 second | 2-10 seconds (relay sync) |
| Push notifications | Instant | Not designed (critical gap) |
| Offline message queuing | Server holds messages | Relay holds events (similar) |
| Background operation | Minimal (push-based wake) | Requires periodic background sync |
| Battery impact | 1-3% daily | 3-15% daily (depending on config) |
| Onboarding | Phone number, 30 seconds | Generate keys, optionally set up cold storage, find WoT peers. Minutes to hours. |
| Key backup | PIN + server-side encrypted backup | Cold storage root key + social recovery |
| Multi-device | Link device via QR code (10 seconds) | Publish kind 10050 with root key (requires cold storage access) |
| Data usage | 50-200 MB/month (messaging) | 45-90 MB/month (comparable) |
| Group messaging | Native, instant delivery | NIP-29 via relay, comparable |
| Contact discovery | Phone number matching | WoT + follows (no phone numbers, better privacy, worse discovery) |
| Media | Inline images, video, voice | External URLs (text-focused) |
| App size | 100-200 MB | Likely 30-80 MB (no server-side media processing) |

**The UX gaps that matter most:**

1. **Push notifications.** Signal/WhatsApp/Telegram deliver messages instantly via push. Gozzip has no push design. This alone makes Gozzip nonviable as a mobile messaging app until solved.

2. **Onboarding.** Signal: enter phone number, verify SMS, done. Gozzip: generate keypair, understand root key vs device key (or have this hidden), find peers to follow, wait for pacts to form. Even with excellent UX hiding the complexity, the "empty network" problem (no contacts, no pacts, bootstrap phase) means the first-run experience is inferior.

3. **Multi-device setup.** WhatsApp: scan QR code on existing device. Gozzip: access root key from cold storage, publish new device delegation. If the root key is on a hardware wallet at home and the user bought a new phone at a store, they cannot set up Gozzip until they get home.

4. **Message delivery reliability.** Signal/WhatsApp have 99.9%+ delivery reliability because centralized servers queue messages. Gozzip's reliability depends on relay availability plus pact partner availability. During Bootstrap phase (0-5 pacts), reliability is entirely relay-dependent, which is equivalent to Nostr's current reliability (variable, relay-dependent).

5. **Contact discovery.** WhatsApp/Signal use phone numbers for contact discovery. Gozzip uses public keys and WoT. This is a privacy advantage but a discoverability disadvantage. "Is my friend on Gozzip?" requires knowing their public key or discovering them through WoT, not just checking a phone number.

### Gap assessment

**Severity: Critical (for mass adoption), Minor (for target audience).**

If Gozzip targets the mass market, the UX gaps versus Signal/WhatsApp are large and primarily in onboarding, push notifications, and device management. These gaps are bridgeable with engineering but require significant UX investment.

If Gozzip targets privacy-conscious users, Nostr community members, and decentralization enthusiasts (the realistic early adopter audience), the gaps are smaller. These users accept complexity trade-offs for sovereignty. The protocol's phased rollout (desktop/extension first, mobile second) correctly prioritizes this audience.

### Suggested fix

1. **Push notifications** are prerequisite for mobile viability. Implement notification relays (see section 5).
2. **Onboarding** should hide key management entirely for new users. Generate root key, encrypt to device, offer optional backup to iCloud/Google. Cold storage migration is an advanced feature, not the default path.
3. **Device linking** should use QR code or link, with an existing authorized device granting temporary delegation to the new device. Root key confirmation within 7 days (or never, for casual users).
4. **Contact discovery** should support NIP-05 identifiers (user@domain.com style) for human-readable addresses, plus optional phone number hashing for friend discovery (privacy-preserving, similar to Signal's approach).
5. Accept that Gozzip's mobile experience will be "Signal minus instant delivery plus data sovereignty." Frame this honestly. Do not claim parity with centralized messengers.

---

## Summary Matrix

| Issue | Protocol Assumption | Mobile Reality | Gap Bridgeable? | Severity |
|-------|-------------------|----------------|-----------------|----------|
| 1. Storage on mobile | <1 GB, 100x margin | 200-500 MB realistic, 40-100x margin | Already fine | Minor |
| 2. Bandwidth on metered | 4.1 MB/day | 1.5-3 MB/day realistic | Already fine | Minor |
| 3. Key management | Cold storage root key | Average users do not own hardware wallets | Yes, with UX tiering | Significant |
| 4. Multi-device sync | Checkpoint reconciliation | Works well, 2-second sync | Already fine | Minor |
| 5. Push notifications | Not addressed | Required for mobile messaging | Yes, with notification relay design | Critical |
| 6. App Store | Not addressed | Navigable with careful engineering | Yes, with implementation care | Significant |
| 7. vs existing messaging | Not compared | Large UX gap in onboarding and push | Partially bridgeable | Critical (mass), Minor (early adopters) |

## Overall Assessment

Gozzip's mobile story has two critical gaps and two significant ones. The critical gaps are:

1. **Push notifications are undesigned.** This is the single biggest blocker for mobile viability. Without push, the app is a polling client that users must manually open to check for messages. No mainstream messaging app works this way.

2. **Comparison to existing messaging apps reveals large UX gaps** in onboarding and push notifications that must be addressed for mass adoption.

The good news: the protocol's core design actually handles mobile well. The 24h challenge windows for intermittent devices, relay-mediated delivery for intermittent-to-intermittent pairs, checkpoint reconciliation, and phased adoption model all correctly accommodate mobile constraints. Storage and bandwidth requirements are modest and within mobile device capabilities.

The phased rollout (desktop/extension first, mobile second) is the correct strategy. By the time mobile is the primary target, Full node infrastructure from desktop users will be established, and the "Keepers carry the load, Witnesses provide redundancy" model will work.

**Bottom line:** Gozzip can work on mobile, but it will be a "check when you open it" app (like email) more than a "instant notification" app (like WhatsApp) until the push notification gap is solved. The protocol should own this honestly and present a concrete plan for notification delivery.
