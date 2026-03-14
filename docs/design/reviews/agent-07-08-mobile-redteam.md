# Agent 07/08: Mobile Constraints & Red Team Review

**Date:** 2026-03-14
**Scope:** Mobile resource constraints, push notification trust model, app store compliance, attack scenario design
**Documents reviewed:** Whitepaper v1.2, push-notifications.md, key-management-ux.md, media-layer.md, monitoring-diagnostics.md, guardian-incentives.md, pact-state-machine.md

---

## Executive Summary

The Gozzip protocol demonstrates careful thinking about mobile participation, separating media from text pacts, defining explicit light-node uptime assumptions, and designing lazy media loading. However, the protocol's mobile viability rests on several assumptions that do not survive adversarial analysis. The push notification architecture introduces a trusted third party whose metadata exposure is structurally worse than acknowledged. The key hierarchy's reliance on secp256k1 software keys --- when iOS and Android both provide hardware-backed Secure Enclave/StrongBox primitives --- creates a key management surface that is both less secure and harder to get through App Store review than necessary. Five attack scenarios (Sybil WoT infiltration, relay cartel metadata reconstruction, DM timing correlation, data hostage, and device compromise cascade) demonstrate that a moderately resourced adversary ($50K--200K budget, 8--12 week timeframe) can compromise user privacy, data availability, or identity integrity. None of these attacks require breaking cryptographic primitives; they exploit protocol-level trust assumptions and the gap between the protocol's theoretical model and mobile operating system realities.

---

## Issues

### M-01: Background Execution Model Is More Constrained Than Acknowledged

**Severity:** HIGH

**Description:** The whitepaper states light node uptime of 30% and acknowledges iOS/Android background limits of 0.3--5%, then claims "the protocol's availability calculations remain valid at these lower figures because full nodes dominate the availability guarantee." This dismissal is incomplete. The 30% uptime figure assumes "the user actively opens the app several times daily" --- a usage pattern that describes power users, not typical messaging app users. Signal and WhatsApp users open their apps when they receive a push notification, not proactively. The median user of a messaging app opens it 5--10 times per day for 1--3 minutes each, producing effective foreground uptime of 0.3--2%.

The real problem is not availability (full nodes handle that) but **challenge-response liveness**. A light node that is awake for 30 minutes per day has approximately 2% uptime. With daily challenges (jittered +/-6 hours), the probability that a challenge arrives during one of these waking windows is roughly 2%. This means most challenges will time out, triggering the channel-aware retry path. If the retry also misses the waking window, the challenge is marked as failed. Over 30 days with alpha=0.95, a light node that can only respond to challenges during foreground sessions will see its reliability score degrade to the 50--70% range, triggering pact degradation and replacement --- even though the node is faithfully storing all data.

**Recommendation:** Implement a **challenge queue with deferred response** mechanism. Challenges received while the app is backgrounded are queued and responded to on the next foreground session or background fetch cycle. The challenge timeout should be extended from 10 seconds to 24 hours for light nodes. The reliability score computation should use a "responded within window" metric rather than "responded within 10 seconds." This requires the pact state machine to distinguish between "light node challenge" and "full node challenge" with different timeout semantics. Without this, the protocol will churn through light-node pacts at an unsustainable rate.

---

### M-02: Push Notification Relay as a Covert Metadata Aggregator

**Severity:** HIGH

**Description:** The push notification design correctly identifies that the notification relay learns timing metadata --- "user X received an event matching filter Y at time T." The document claims this is "the same privacy model Signal uses." This comparison is misleading. Signal operates its own push infrastructure and is the only entity seeing push metadata for Signal messages. In Gozzip's architecture, the notification relay is operated by a **third party** (community operator, relay operator, or the Gozzip project) who is not the messaging platform itself.

The metadata exposure is structurally more severe than acknowledged:

1. **DM timing correlation.** The notification relay knows the user's pubkey and that a kind 14 event arrived at time T. If the relay also handles notifications for the sender (or operates a Nostr relay that sees the outbound DM), it can correlate sender and recipient timing to reconstruct the DM communication graph with high confidence.

2. **Filter preferences as behavioral profile.** The kind 10062 registration reveals which event kinds the user cares about (DMs, mentions, replies, reactions). Combined with push delivery timing, this reveals the user's social activity pattern: "User X receives DMs at 23:00--02:00 (late night conversations), mentions peak Tuesday afternoons (work-related posting)."

3. **Quiet hours as geolocation proxy.** The quiet hours field includes a UTC offset. While the document notes "the offset, not the timezone name," a UTC offset narrows the user's location to a band approximately 1,000 km wide. Combined with push timing patterns (activity peaks corresponding to local business hours), this provides a coarse geolocation estimate.

4. **Token-to-pubkey binding as persistent identifier.** Even with 30-day token rotation, the notification relay maintains a persistent pubkey-to-device mapping. Over months, this builds a longitudinal profile of the user's device usage patterns, sleep schedule, and social activity.

The mitigation of "register with 2--3 notification relays" actually **increases** the metadata surface: each relay independently learns the same information, and if any two relays share data (or are operated by the same entity under different names), the redundancy provides correlation confirmation rather than privacy.

**Recommendation:** (a) Make quiet hours timezone offset coarse-grained (round to 3-hour increments, reducing geographic precision to ~4,500 km bands). (b) Require the notification relay to implement a **token-blind delivery** model: the push token is encrypted to the notification relay, but the pubkey used for the relay subscription is an ephemeral, unlinkable key rotated with each registration cycle. The notification relay subscribes to events for ephemeral keys, not root pubkeys. (c) Specify a protocol-level requirement that notification relay operators MUST NOT also operate standard Nostr relays, or if they do, MUST implement organizational separation with independent key management. (d) Document the metadata exposure honestly --- the current framing ("same as Signal") understates the risk.

---

### M-03: secp256k1 Software Keys vs. Hardware-Backed Secure Enclave

**Severity:** HIGH

**Description:** The protocol mandates secp256k1 keypairs for identity, inherited from Nostr. The key management UX document describes storing the root key encrypted with device biometrics (Face ID / Touch ID), backed up to iCloud Keychain or Google Cloud Keystore. This is a software key protected by hardware authentication --- not a hardware-backed key.

Modern iOS devices provide the Secure Enclave, which generates and stores P-256 (secp256r1) keys that **never leave the hardware**. The private key material cannot be exported, backed up, or read by any software --- not even the OS. Android's StrongBox provides equivalent guarantees with P-256 or Ed25519 keys.

The protocol's secp256k1 choice means:

1. **The root key exists as extractable bytes in memory.** During any signing operation, the secp256k1 private key is loaded into app process memory. A memory-reading exploit (or a jailbroken device) can extract it.

2. **Cloud backup creates a second attack surface.** The root key in iCloud Keychain is encrypted, but it is accessible to Apple (under court order) and to anyone who compromises the user's Apple ID. This is a fundamentally different security model from Secure Enclave keys, which cannot be cloud-backed because they cannot be extracted.

3. **App Store review friction.** Apple's App Review guidelines (section 2.1) scrutinize apps that implement custom cryptography. An app that stores "master identity keys" protected only by software encryption will face questions about why it does not use the platform's hardware security module. This is not a theoretical concern --- cryptocurrency wallet apps have been rejected or delayed for similar reasons.

4. **Key derivation prevents hardware key usage.** The HKDF-SHA256 derivation scheme (IKM = root private key) requires the root private key as input. If the root key were in a Secure Enclave, derivation would be impossible because the private key cannot be read. The entire key hierarchy is structurally incompatible with hardware-backed key storage.

**Recommendation:** Introduce a **dual-key architecture** for device subkeys. The device subkey is a Secure Enclave P-256 key (hardware-backed, non-extractable). The root key remains secp256k1 for Nostr compatibility. The kind 10050 device delegation event binds the Secure Enclave public key to the root identity via a root-key signature. Day-to-day event signing uses the Secure Enclave key (P-256 signature in a `device_sig` tag), while the root key is used only for governance operations (profile changes, device delegation, DM key rotation) and can remain in cold storage. This provides hardware-grade protection for the most frequent operation (event signing) while maintaining Nostr key compatibility. Clients that receive events with both secp256k1 and P-256 signatures can verify either. The cost is an additional signature per event (~64 bytes) and a new verification path.

---

### M-04: Mobile Battery Impact Is Underestimated for Active Pact Participation

**Severity:** MEDIUM

**Description:** The whitepaper claims 1--3% battery for background sync and 10--15% for foreground active use. These estimates do not account for the full protocol workload on a light node with 14--20 active pacts:

1. **Challenge response computation.** Each incoming challenge requires reading a range of events from local storage, serializing them in canonical JSON format, concatenating with the nonce, and computing SHA-256. For a range of 7 events at 925 bytes each, this is ~6.5 KB of data processing per challenge. With 14--20 pact partners each sending 1 challenge per day, that is 14--20 SHA-256 computations per day --- trivial in isolation, but each requires waking the app from background, reading from disk, and performing CPU work.

2. **Event sync to 14--20 partners.** Publishing an event requires forwarding it to all pact partners via NIP-46 encrypted relay channels. Each forward involves NIP-44 encryption (X25519 + ChaCha20-Poly1305), a WebSocket write, and waiting for relay acknowledgment. For 30 events/day to 20 partners, that is 600 encrypted WebSocket writes per day.

3. **Checkpoint verification.** Merkle root computation over all events since the last checkpoint requires reading and hashing potentially thousands of events.

4. **Persistent WebSocket connections.** Maintaining connections to 3--5 relays for NIP-46 channels burns battery continuously. iOS kills WebSocket connections aggressively after 30 seconds of backgrounding. Reconnecting requires TCP handshake + TLS handshake + WebSocket upgrade + NIP-42 auth --- each reconnection costs ~5 KB of data and ~200 ms of radio active time.

Realistic battery impact for a light node with 15 pacts, 30 events/day, and 5 relay connections is closer to **5--8% per day** for background operations, and **15--25%** with foreground use. On a 4,000 mAh battery, 8% is 320 mAh --- roughly equivalent to 30 minutes of screen-on web browsing. Users will notice this.

**Recommendation:** (a) Implement **batched sync windows** rather than per-event forwarding. Queue events locally and sync to pact partners in a single burst every 15--60 minutes. This amortizes connection establishment costs. (b) Reduce the default pact count floor for mobile-only users from 12 to 8, accepting slightly lower availability in exchange for reduced battery drain. (c) Specify a **power-aware mode** where the client reduces pact partner sync frequency to 2--4 times per day when battery drops below 20%. (d) Use relay connection pooling: maintain 1--2 persistent connections and multiplex all NIP-46 traffic through them, rather than maintaining per-partner connections.

---

### M-05: App Store Compliance Risks for iOS

**Severity:** MEDIUM

**Description:** Several protocol features create App Store review friction:

1. **Peer-to-peer data storage.** Apple's guidelines (section 5.2.3) require apps to explain why they store data on behalf of other users. The pact mechanism stores other users' events on the device. Apple may interpret this as the app turning the user's device into a server --- a characterization that has led to rejections of BitTorrent-adjacent apps.

2. **BLE mesh networking.** Using Core Bluetooth for mesh message relay (Tier 0) requires the `bluetooth-central` and `bluetooth-peripheral` background modes. Apple scrutinizes apps requesting both capabilities. The 7-hop relay behavior (forwarding data for users the device owner does not know) is exactly the pattern Apple has questioned in mesh networking apps like Bridgefy and Briar.

3. **Background processing abuse.** The protocol requires background execution for challenge responses, event sync, and push notification processing. Apple's BGAppRefreshTask is designed for content refresh (fetching new data), not for serving data to other clients. Using background execution to respond to challenge-response proofs or serve pact data may violate Apple's intended use of background APIs.

4. **Push notification certificate requirements.** The notification relay requires an Apple Developer account ($99/year) and APNs certificate provisioning. The current design assumes the notification relay operator holds the APNs certificate. If the Gozzip project operates the default notification relay, it must maintain Apple Developer Program enrollment and comply with Apple's push notification policies, including content restrictions.

5. **Encryption export compliance.** Apps using custom encryption (NIP-44, the key hierarchy, challenge-response hashing) must file an annual self-classification report with the US Bureau of Industry and Security (BIS) and declare encryption usage in App Store Connect. The 4-level key hierarchy and HKDF derivation exceed the complexity threshold for "standard encryption" exemptions.

**Recommendation:** (a) Design the iOS app with a **compliant-by-default** configuration: BLE mesh disabled by default (opt-in), pact storage framed as "backup for your contacts" (user benefit, not network service), background processing limited to content refresh (not challenge-response serving). (b) Prepare a detailed App Review explanation document covering each capability and its user benefit. (c) File BIS encryption self-classification proactively. (d) Consider shipping the initial iOS release without Tier 0 (BLE mesh) to reduce review surface, adding it in a later update after establishing review history.

---

### M-06: Mobile Storage Pressure from Pact Obligations

**Severity:** MEDIUM

**Description:** The whitepaper claims 200--500 MB storage for a power user. The media layer document specifies a 500 MB media cache. Combined with pact storage obligations:

- Text pact storage: 20 partners x 675 KB/month x 30-day window = 13.5 MB (manageable)
- Media cache: 500 MB default
- Read cache: 100 MB default
- Diagnostic data: 2--5 MB (up to 15 MB with developer mode)
- Own event history: variable
- SQLite indexes and metadata: ~20% overhead on stored data

Total baseline: approximately 620--650 MB for a standard user. This is reasonable on a 128 GB device (0.5%), but the protocol does not specify **eviction behavior under storage pressure**.

iOS sends `UIApplication.didReceiveMemoryWarning` and can terminate apps that exceed their storage allocation. If the OS reclaims storage by purging the app's caches, pact-stored events for partners are lost. The next challenge for those events will fail, degrading the reliability score. If this happens systematically (device near full storage), the user's pact health degrades through no fault of their own.

**Recommendation:** (a) Implement a **storage pressure response** in the pact state machine. When device storage drops below 1 GB free, begin LRU eviction of read-cache data (not pact-obligated data). Below 500 MB, evict old pact-stored events (oldest events for partners with the most replicas elsewhere). Below 200 MB, notify the user and offer to reduce pact count. (b) Mark pact-stored events as `NSURLIsExcludedFromBackupKey = YES` on iOS to prevent iCloud from counting them against the user's cloud storage quota. (c) Specify maximum storage budgets per pact tier in the protocol: light nodes MUST NOT store more than 50 MB per pact partner (30-day window cap).

---

### ATK-01: Sybil WoT Infiltration Attack

**Severity:** CRITICAL

**Description:** An adversary creates 50 Nostr identities over 8--12 weeks with the goal of infiltrating a target's Web of Trust to form malicious pacts and gain access to the target's stored data.

**Attack sequence:**

1. **Week 1--2: Identity creation ($500).** Create 50 secp256k1 keypairs. Publish kind 0 profiles with plausible metadata. Begin posting kind 1 notes with topically relevant content (generated or human-curated). Cost: VPS hosting for 50 accounts, content generation.

2. **Week 2--4: Social graph infiltration ($2,000).** Follow 200--500 real users in the target's community. Engage authentically: reply to posts, react, share content. Build genuine social connections with the target's 2-hop WoT peers. At this stage, the adversary's accounts are indistinguishable from enthusiastic newcomers.

3. **Week 4--6: WoT edge establishment.** Some portion of the 200--500 followed users will follow back (typical reciprocal follow rate in Nostr communities: 10--30%). This produces 20--150 mutual follows per Sybil account, establishing 1-hop WoT distance to many community members.

4. **Week 6--8: Target approach.** Sybil accounts that have achieved mutual follows with the target's close contacts begin following the target. If the target follows back (either manually or via WoT-recommendation UI), the Sybil account is now at WoT distance 1. If not, it is at distance 2 via mutual friends --- still within pact formation range.

5. **Week 8--10: Pact formation.** The follow-age requirement is 30 days of mutual follow history. By week 8, accounts created in week 1 have exceeded this threshold. The 7-day minimum account age was passed in week 2. Volume matching is straightforward: the Sybil account posts at a similar rate to the target.

6. **Week 10--12: Data access.** With active pacts, the adversary receives and stores the target's events. Challenge-response verification works correctly --- the adversary's server dutifully stores and serves the data. The adversary now has a continuously updated copy of the target's entire event history (or 30-day window for light pacts).

**Cost estimate:** $5,000--15,000 for 50 identities over 12 weeks (VPS hosting, content generation, operational management). A state-level adversary could run this at scale for thousands of targets.

**Why existing defenses are insufficient:**
- The 7-day account age minimum is trivially exceeded.
- The 30-day follow-age requirement is exceeded by week 8.
- Volume matching is trivially gamed.
- The WoT distance check (2-hop) is defeated by genuine community engagement.
- There is no mechanism to detect coordinated Sybil identity creation --- the accounts are operationally independent.

**Recommendation:** (a) Increase the follow-age requirement from 30 days to 90 days for pact formation (except bootstrap pacts). This pushes the attack timeline to 14--16 weeks, significantly increasing adversary cost and exposure risk. (b) Introduce a **pact diversity constraint**: no more than 30% of a user's active pacts may be with accounts created within the same 30-day window. This forces Sybil campaigns to stagger identity creation over months. (c) Implement a client-side heuristic: flag pact candidates whose follow graph has high overlap with other candidates (Jaccard similarity > 0.5 among follow lists) as potential Sybil clusters. (d) Consider requiring a minimum **interaction history** (replies, DMs, or reactions exchanged with the target) before pact formation, not just follow-age.

---

### ATK-02: Relay Cartel Attack --- Social Graph Reconstruction

**Severity:** CRITICAL

**Description:** The whitepaper acknowledges that "colluding relay operators controlling 60--80% of relay traffic can reconstruct near-complete social graphs." This section develops the attack in detail.

**Precondition:** 5 relay operators collude (or a single entity operates 5 relays under different brands). In Nostr's current ecosystem, the top 5--10 relays handle a disproportionate share of traffic. The protocol does not change this --- it uses standard Nostr relays.

**Attack surface:**

1. **NIP-46 channel metadata.** Every pact operation (formation, challenge-response, event sync) flows through NIP-46 encrypted relay channels. The relay cannot read the content, but it sees: sender pubkey, recipient pubkey (via `p` tag), timestamp, and message size. From pact formation events (kind 10053, 10055, 10056), the relay can identify pact pairs. From challenge events (kind 10054), the relay can confirm which pairs have active pacts and their challenge frequency (indicating pact health).

2. **Rotating request token reversal.** Data requests (kind 10057) use tokens computed as `SHA-256(target_pubkey || date)`. The whitepaper acknowledges this is "trivially reversible by any party with knowledge of the target's public key." A relay handling kind 10057 events can precompute tokens for all known pubkeys (a few seconds of computation for a million pubkeys) and identify which author is being requested. Combined with the requester's pubkey, this reveals "who reads whom" --- the consumption graph.

3. **DM delivery timing.** The relay sees NIP-59 gift-wrapped DMs (sender device pubkey in the event's `pubkey` field, recipient in the `p` tag). The device pubkey is resolvable to a root identity via kind 10050. By correlating DM send and receive times across relays, the cartel reconstructs the complete DM communication graph.

4. **Push notification timing.** If the notification relay is operated by or shares data with the relay cartel, kind 10062 registrations reveal which pubkeys have mobile devices. Kind 20062 wake-up hints from pact partners reveal event arrival times.

**Reconstruction capabilities with 60--80% relay coverage:**
- Complete follow graph (kind 3 events are public)
- Pact topology (NIP-46 channel metadata)
- Content consumption graph (rotating token reversal + request timing)
- DM communication graph (NIP-59 metadata)
- Device inventory per identity (kind 10050)
- Geographic timezone band (push notification quiet hours)
- Activity schedule (event publish times + push delivery times)

**Cost estimate:** Operating 5 Nostr relays costs approximately $500--2,000/month total. The metadata analysis infrastructure (database, correlation engine) adds $1,000--5,000 for initial development. Ongoing cost: $2,000--5,000/month. This is within reach of any intelligence agency, well-funded corporation, or motivated individual.

**Recommendation:** (a) **NIP-59 gift-wrap all pact protocol events** (kinds 10053, 10054, 10055, 10056, 10057, 10058), not just DMs. This hides the sender's pubkey from the relay, replacing it with an ephemeral key. The `p` tag is also wrapped, hiding the recipient. The relay sees only the outer wrapper's ephemeral pubkey. Cost: one additional X25519 + ChaCha20 encryption per protocol event. (b) **Implement relay diversity enforcement in the client**: pact channels for any given pair should use at least 2 different relays (selected from different operators). Rotate relay assignment every 30 days. (c) **Replace the rotating request token** with a proper private information retrieval (PIR) scheme or, at minimum, a token that is not deterministically computable from the target pubkey alone (e.g., `SHA-256(target_pubkey || date || requester_secret)`). The current token provides no meaningful privacy against the most obvious adversary (the relay).

---

### ATK-03: DM Timing Attack via NIP-59

**Severity:** HIGH

**Description:** NIP-59 gift wraps encrypt DM content but expose metadata to the relay. The whitepaper acknowledges: "NIP-59 gift wraps expose both sender (device pubkey, resolvable to root identity via kind 10050) and recipient (p tag) to relay operators handling DM traffic."

The timing attack is straightforward:

1. **Observation.** An adversary monitoring a relay sees Alice publish a gift-wrapped event to Bob at 14:03:22. Bob's device fetches the event at 14:03:45 (23 seconds later, via push notification wake-up).

2. **Correlation.** If the adversary also monitors the notification relay (or operates it), they see a push notification sent to Bob at 14:03:24. The timing correlation (event published -> push sent -> device wakes -> event fetched) produces a signature with sub-minute precision.

3. **Pattern analysis.** Over weeks, the adversary builds a timing profile: Alice messages Bob at 22:00--23:30 UTC most evenings. Alice messages Carol at 09:00--12:00 UTC on weekdays. This reveals relationship patterns, schedules, and communication intensity without reading any content.

4. **Active probing.** The adversary sends DMs to Alice from a controlled identity at varying times and measures Bob's wake-up pattern. If Bob consistently wakes up within 30 seconds of Alice receiving a DM from the probe account, the adversary can infer that Alice forwards information to Bob in real-time.

**Mitigation limitations:** The push notification document suggests "DM relay rotation and batched delivery can partially mitigate timing analysis." Batching with a 60-second cooldown reduces timing precision but does not prevent long-term pattern analysis. Relay rotation changes which relay sees the metadata but does not eliminate the metadata itself.

**Recommendation:** (a) Implement **random delay injection** for DM push notifications: add 0--300 seconds of random delay before sending a push for DM events. This degrades real-time delivery UX but prevents sub-minute timing correlation. Make the delay configurable (0 for users who prioritize speed, 300 for users who prioritize privacy). (b) **Decouple DM relay from social relay.** DMs should be routed through a different relay than the user's NIP-65 social relays. This prevents a single relay from seeing both the social graph and the DM graph. (c) Implement **DM batching at the sender**: accumulate outbound DMs for 1--5 minutes and send as a batch, preventing per-message timing analysis.

---

### ATK-04: Data Hostage Attack

**Severity:** HIGH

**Description:** A malicious pact partner stores the target's data faithfully for months, passes all challenges, and achieves a high reliability score. The partner then simultaneously:

1. Deletes all stored data for the target.
2. Responds to challenges with incorrect hashes (or stops responding entirely).
3. If the adversary controls multiple pact partners (via Sybil infiltration, ATK-01), drops all of them simultaneously.

**Impact analysis:**

- **Single hostile partner:** Minimal impact. The pact transitions to Failed -> Dropped. Standby partner promoted. Data loss: zero (other partners hold copies).

- **Coordinated drop of 3--5 partners:** Moderate impact. Multiple simultaneous failures may trigger partition detection (if the adversary's identities are in the same WoT cluster). If they are distributed across WoT clusters, partition detection does not activate, and the client must find 3--5 replacement pact partners while operating at reduced redundancy. During this window (potentially 48+ hours for pact replacement), the user's data availability drops.

- **Coordinated drop of 8+ partners (via successful Sybil infiltration):** Severe impact. If the adversary controls 8 of 20 active pact partners, a simultaneous drop reduces the user to 12 active partners. If 4 of the remaining 12 are light nodes that are currently offline, the user has 8 active sources. The availability probability drops from 99.97% to approximately 95--99%. The user is functionally degraded until 8+ replacement pacts are formed --- which takes days to weeks given the 48-hour offer evaluation window and follow-age requirements.

- **Data hostage variant.** Instead of deleting data, the adversary holds the data and demands payment (in bitcoin or other consideration) for continued storage. The adversary stops responding to challenges but claims to still hold the data. The target cannot verify this claim without the challenge-response succeeding. If the adversary controls enough pact partners that the remaining honest partners cannot reconstruct the complete history, the target faces partial data loss.

**Recommendation:** (a) Implement **data recovery from read-caches**. When a pact partner drops, the client should immediately attempt to reconstruct the lost data by querying all followers (who may have read-cached the events) and relays (which may have stored the events). (b) The pact state machine should trigger **emergency pact formation** when 3+ pacts drop simultaneously: reduce the follow-age requirement to 7 days, expand the WoT search radius to 3 hops, and accept pacts from standby partners without full qualification checks. (c) Implement **pact partner independence scoring**: track the correlation between pact partners' online/offline patterns. If multiple partners show suspiciously correlated behavior (joining within the same week, same challenge-response timing, same uptime pattern), flag them as potentially coordinated.

---

### ATK-05: Device Compromise Cascade

**Severity:** HIGH

**Description:** The key management UX document describes a scenario where the root key is stored encrypted by device biometrics, backed up to iCloud Keychain or Google Cloud Keystore. This creates a cascade of compromise vectors:

**Scenario 1: Single device compromise.**
An adversary with physical access to an unlocked device (or a remote exploit achieving code execution) can:
- Extract the root key from app memory during any signing operation
- Read the device subkey from the Keychain/Keystore
- Access all locally stored pact partner events
- Sign events as the user
- Derive all DM decryption keys (current and past, since HKDF derivation from root key is deterministic given the rotation epoch)

The key management document acknowledges temporary delegation is "bounded damage" (7-day window). But root key extraction is unbounded damage: the adversary can derive all past and future DM keys, create new device delegations, and modify the follow list.

**Scenario 2: Apple ID / Google account compromise.**
If the adversary compromises the user's Apple ID (phishing, SIM swap, password reuse), they gain access to iCloud Keychain, which contains the root key. From a new device:
- Restore the root key from iCloud Keychain
- Derive all keys in the hierarchy
- Publish a new kind 10050 adding an adversary-controlled device
- Begin receiving all the user's events and DMs

The 7-day temporary delegation window does not apply here because the adversary has the root key itself, not just a delegated device key.

**Scenario 3: Cloud provider cooperation.**
Law enforcement with a valid court order can compel Apple or Google to provide iCloud Keychain or Google Cloud Keystore contents. This yields the root key and complete identity compromise. Unlike Signal (where server compromise reveals no message content), Gozzip root key compromise reveals the user's complete identity, social graph, and DM decryption capability.

**Recommendation:** (a) **Never store the root key in cloud-syncable keychain** as the default behavior. Standard mode should store the root key in the device's Secure Enclave-backed keychain (`kSecAttrAccessibleWhenUnlockedThisDeviceOnly` on iOS), which is not synced to iCloud. Cloud backup should be opt-in, not default, with clear warnings. (b) Implement **DM key ratcheting** independent of the root key. Use a Signal-style double-ratchet for DM conversations so that root key compromise does not retroactively decrypt past DMs. The current HKDF-based DM key derivation provides rotation but not forward secrecy against root key compromise. (c) Add a **compromise detection mechanism**: if a new device delegation appears in kind 10050 that the user did not initiate, the client on existing devices should alert immediately and offer one-tap revocation via the governance key.

---

### ATK-06: Guardian Abuse --- Silent Censorship of Seedlings

**Severity:** MEDIUM

**Description:** A malicious Guardian volunteers to store a Seedling's data but selectively drops or modifies events. The Seedling has no way to verify what the Guardian is storing --- challenge-response only confirms data possession, not data completeness.

**Attack scenario:**

1. Adversary operates a Sovereign-phase node and volunteers as a Guardian.
2. Seedling is matched and begins publishing events through the Guardian.
3. Guardian stores events selectively: drops events mentioning certain topics, events tagged with certain hashtags, or events interacting with certain pubkeys.
4. Seedling's events that the Guardian drops are effectively censored during the bootstrap period when the Seedling has no other pact partners to provide redundancy.
5. Because the Seedling also publishes to relays (Bootstrap phase), the censorship only affects the pact-layer data availability. But if the Seedling's relays also fail or drop the events, the Guardian's selective storage is the only remaining copy.

**With multiple guardians (Mechanism 4):** The attack is mitigated if the Seedling has 2--3 Guardians. However, a motivated adversary could volunteer multiple Guardian identities (each Sovereign-phase node may volunteer one guardian pact) and capture multiple guardian slots for the same Seedling.

**Guardian reputation gaming:** The adversary completes several legitimate guardianships (kind 10066 completion events) to build guardian reputation, then selectively censors politically targeted Seedlings. The reputation signal makes the adversary appear trustworthy.

**Recommendation:** (a) **Cross-guardian checkpoint verification.** If a Seedling has 2+ Guardians, the client should periodically compare the event sets held by each Guardian (via checkpoint Merkle roots). Divergence indicates selective storage by one Guardian. (b) **Require Guardians to be within the Seedling's 2-hop WoT** after the first 7 days --- i.e., the Seedling should follow someone who follows the Guardian, providing at least one social accountability link. (c) **Guardian assignment should be random and verifiable**, not self-selected. The Seedling should not be able to choose their Guardian (preventing the adversary from targeting specific Seedlings), and the Guardian should not be able to choose their Seedling (preventing targeting from the Guardian side). Use a deterministic matching algorithm based on pubkey hashes.

---

### ATK-07: Relay-Mediated Eclipse Attack on Bootstrap Users

**Severity:** MEDIUM

**Description:** A user in the Bootstrap phase (0--5 pacts) is fully relay-dependent. If the adversary controls the relays the user connects to (via DNS manipulation, BGP hijacking, or simply being the most popular relay in the user's geographic region), the adversary can:

1. **Suppress pact formation.** Drop kind 10055 (pact request) and kind 10056 (pact offer) events, preventing the user from ever forming pacts. The user remains permanently in Bootstrap phase.

2. **Serve selective social graph.** Show the user a curated follow graph that excludes certain pubkeys or communities. Since the Bootstrap user has no pact-layer data to cross-reference, they have no way to detect missing events.

3. **Delay pact maturation.** Allow pact formation but drop challenge-response events (kind 10054), causing all pacts to degrade and fail. The user forms pacts, watches them degrade, and concludes the protocol is broken.

4. **Isolate from notification infrastructure.** Drop kind 10067 (notification relay advertisement) events, preventing the user from discovering notification relays. The user never receives push notifications and concludes the app is unreliable.

The existing channel-aware challenge retry mitigates (3) partially --- the client retries via an alternative relay. But if the adversary controls the top 2--3 relays in the user's NIP-65 list, retries also fail.

**Recommendation:** (a) **Hard-code 2--3 bootstrap relay URLs** in the client that are operated by the project or trusted community members, ensuring a minimum relay diversity floor. (b) **Implement relay health cross-checking**: the client should periodically fetch its own events from a relay it does not normally use and compare with what its primary relays serve. Divergence indicates selective serving. (c) During Bootstrap phase, the client should connect to a minimum of 5 relays (not the user's NIP-65 choice alone) to reduce single-operator control.

---

### ATK-08: Challenge-Response Gaming for Light Nodes

**Severity:** LOW

**Description:** A malicious light node pact partner can pass challenges without storing the full 30-day event window by exploiting the age-biased challenge distribution. The distribution is: 50% oldest third, 30% middle third, 20% newest third. A partner who stores only the events that fall within the most likely challenge ranges and fetches others on-demand can maintain a >90% reliability score while storing only 30--50% of the obligated data.

The serve challenge (requesting a specific event by sequence number with latency measurement) is designed to catch this --- consistently slow responses (>500ms) suggest proxying. But on a modern connection, fetching a 925-byte event from a relay takes 50--200ms, well within the 500ms threshold. The partner can:

1. Store the oldest third of events (50% of challenges target these).
2. Store the newest third of events (likely to be requested by the user).
3. Proxy the middle third from relays on demand.
4. Pass >85% of challenges while storing only ~65% of the obligated data.

**Recommendation:** (a) **Randomize the age-bias distribution** per challenge, so the partner cannot predict which third will be targeted. For example, rotate between (50/30/20), (20/50/30), and (30/20/50) distributions randomly. (b) **Tighten the serve challenge latency threshold** from 500ms to 200ms for partners on broadband connections (detected via historical latency). (c) Periodically issue **bulk verification challenges** that require the partner to produce a Merkle root over the entire stored range --- this can only be computed with possession of all events.

---

## Summary Table

| ID | Title | Severity | Category | Effort to Fix |
|----|-------|----------|----------|---------------|
| M-01 | Background execution challenge-response liveness | HIGH | Mobile | Medium (protocol change) |
| M-02 | Push notification relay metadata aggregation | HIGH | Privacy/Mobile | High (architectural change) |
| M-03 | secp256k1 software keys vs. Secure Enclave | HIGH | Security/Mobile | High (dual-key architecture) |
| M-04 | Battery impact underestimated | MEDIUM | Mobile | Medium (implementation) |
| M-05 | App Store compliance risks | MEDIUM | Mobile/Deployment | Medium (design + documentation) |
| M-06 | Mobile storage pressure handling | MEDIUM | Mobile | Low (implementation) |
| ATK-01 | Sybil WoT infiltration (50 identities, 12 weeks, $15K) | CRITICAL | Attack | High (protocol change) |
| ATK-02 | Relay cartel social graph reconstruction (5 relays, $5K/mo) | CRITICAL | Attack/Privacy | High (NIP-59 wrapping for all protocol events) |
| ATK-03 | DM timing attack via NIP-59 | HIGH | Attack/Privacy | Medium (delay injection) |
| ATK-04 | Data hostage attack (coordinated pact drop) | HIGH | Attack | Medium (emergency formation + recovery) |
| ATK-05 | Device compromise cascade via cloud keychain | HIGH | Attack/Security | High (key storage architecture change) |
| ATK-06 | Guardian silent censorship of Seedlings | MEDIUM | Attack | Medium (cross-guardian verification) |
| ATK-07 | Relay-mediated eclipse of Bootstrap users | MEDIUM | Attack | Low (bootstrap relay hardcoding) |
| ATK-08 | Challenge-response gaming by light nodes | LOW | Attack | Low (distribution randomization) |

---

## Overall Assessment

The Gozzip protocol is architecturally sound in its ambitions but has significant gaps at the intersection of mobile device realities and adversarial conditions. The most pressing issues are:

1. **The key management architecture is structurally incompatible with hardware security on mobile.** The secp256k1 + HKDF derivation design forces all keys into software, forgoing the strongest security primitive available on modern phones (Secure Enclave / StrongBox). This is not a niche concern --- it affects every mobile user and creates App Store review friction. A dual-key architecture (secp256k1 root for Nostr compatibility, P-256 device keys in hardware) would resolve this without sacrificing protocol compatibility.

2. **The push notification relay's metadata exposure is worse than Signal's model and the documentation understates this.** Signal operates its own push infrastructure; Gozzip delegates to third-party notification relays. The quiet-hours timezone offset, DM timing correlation, and token-to-pubkey binding combine to create a surveillance surface that the "same as Signal" framing obscures. This needs honest documentation and architectural mitigation (ephemeral subscription keys, coarse timezone offsets).

3. **The Sybil WoT infiltration attack is practical and affordable.** The 30-day follow-age requirement and 7-day account age minimum are insufficient barriers for a motivated adversary. Increasing follow-age to 90 days, adding pact diversity constraints, and implementing Sybil cluster detection are necessary for any deployment facing state-level adversaries.

4. **The relay cartel attack is the protocol's deepest structural vulnerability.** By inheriting Nostr's relay model, the protocol inherits the concentration risk of the existing relay ecosystem. NIP-59 wrapping all protocol events (not just DMs) is the minimum viable mitigation, but it adds encryption overhead to every pact operation.

5. **Mobile battery and background execution constraints will define user experience.** The difference between "1--3% battery" and the realistic "5--8% battery" is the difference between invisible and annoying. Batched sync, connection pooling, and power-aware mode are implementation necessities, not optimizations.

None of these issues are fatal. The protocol's core insight --- that the social graph can serve as storage infrastructure --- remains valid. But the gap between the whitepaper's theoretical analysis and mobile deployment reality is wider than the current documentation acknowledges. Closing this gap requires treating mobile constraints and adversarial conditions as first-order design inputs, not as edge cases to be addressed after the core protocol is specified.
