# Red Team Analysis: Gozzip Protocol

**Date:** 2026-03-12
**Analyst:** Adversarial review (red team)
**Scope:** 10 attack scenarios against the Gozzip protocol as specified in the whitepaper (v1.1), ADRs 006-008, protocol specs, and design documents.

---

## Attack 1: Sybil WoT Infiltration and Eclipse

### Objective

Create 50 fake identities over 2 months, embed them in a target community's Web of Trust, then use the accumulated position to eclipse a specific user -- controlling their pact partner set and censoring or manipulating their data availability.

### Step-by-Step Attack Plan

1. **Week 1-2: Identity creation.** Generate 50 secp256k1 keypairs. Stagger creation over 14 days (3-4 per day) to avoid obvious batch patterns. Each identity gets a plausible profile (kind 0), a unique posting pattern, and is published to a spread of relays. The 7-day account age minimum means the first identities become pact-eligible by day 8.

2. **Week 2-4: Organic behavior seeding.** Each Sybil posts 2-5 kind 1 events per day with plausible content (reposts, reactions to real community content). This builds a posting history and makes the accounts look alive. Total effort: scripting + content curation, approximately 4 hours/day for an operator running all 50.

3. **Week 3-6: WoT infiltration.** Each Sybil follows 10-15 real members of the target community. The attacker waits for mutual follows. Key insight: Sybils follow *different* community members to maximize coverage. If the target community has 200 members, 50 Sybils each following 15 members covers up to 150 unique community members. Social engineering (replying helpfully, zapping popular posts) accelerates mutual follows.

4. **Week 4-8: Pact formation.** Sybils whose accounts are 7+ days old and have mutual follows begin broadcasting kind 10055 pact requests with volume parameters matching the target user. Because pact formation requires WoT membership, the attacker needs Sybils that are either followed by the target or are 1-hop WoT peers. The attacker concentrates effort on getting the target to follow 11+ Sybils (directly or via strong 2-hop presence).

5. **Week 6-8: Pact slot occupation.** As the target forms pacts (via kind 10053), Sybils accept aggressively. They actually store the target's data and pass challenge-response (kind 10054) honestly. The goal is to occupy 11+ of the target's 20 active pact slots. Sybils maintain reliable behavior to keep reliability scores above 90%.

6. **Week 8+: Eclipse execution.** Once 11+ pact slots are attacker-controlled:
   - **Selective data withholding:** Refuse to serve certain events to gossip requests (kind 10057) while still passing hash challenges for those events. The target's controversial posts vanish from the gossip layer.
   - **Gossip censorship:** Sybil pact partners, who receive the target's events as pact partners, decline to forward them via gossip. Since pact partners are highest-priority gossip forwarders, this degrades the target's reach.
   - **Challenge timing attack:** When the target challenges a Sybil, the other Sybils simultaneously go offline, creating the impression that many pact partners are unreliable. The target replaces them -- but the replacement pool is also full of Sybils.

### Cost Estimate

- **Time:** 8-12 weeks of sustained effort.
- **Money:** Minimal direct cost. 50 keypairs are free. Relay publishing is free on most relays. If paid relays are used, approximately $250-500 total (50 accounts x $5-10/relay). Social engineering labor is the primary cost -- approximately 160-240 hours of operator time.
- **Expertise:** Moderate. Requires scripting, social engineering, patience. No cryptographic skill needed.

### Protocol Defenses

1. **7-day account age minimum.** Speed bump only. Trivially satisfied by creating accounts early.
2. **WoT membership required for pacts.** This is the real barrier. The attacker must build genuine social relationships. But "genuine" is subjective -- following, reacting, and reposting is enough for most users to follow back.
3. **Volume matching (30% tolerance).** Sybils must match the target's volume. If the target produces 50MB, each Sybil needs ~35-65MB. This requires Sybils to be active posters -- adding to the operational cost.
4. **WoT cluster diversity.** The protocol recommends selecting pact partners from diverse WoT clusters. If implemented, this makes it harder for Sybils from a single operator to dominate. But "cluster diversity" is a client-side heuristic -- not enforced by the protocol. If the client's diversity algorithm is weak, this defense fails.
5. **Standby pact promotion.** 3 standby pacts provide failover. But the attacker can occupy standby slots too (23 total slots, attacker needs 12+).

### Verdict

| Dimension | Rating |
|-----------|--------|
| Exploitability | **Moderate** |
| Impact | **High** |

The protocol's primary defense -- WoT membership -- is a social barrier, not a cryptographic one. The 7-day age requirement is trivially bypassed. The real question is whether the target community is socially vigilant enough to avoid following 11+ Sybils. In tight-knit communities (high clustering coefficient), this is genuinely hard. In loose communities with promiscuous follow behavior, this is very achievable within 2 months.

### Suggested Mitigations

1. **Minimum mutual-follow age for pact formation.** Require that the mutual follow relationship (not just the account) be at least 30 days old before a pact can form. This quadruples the attack timeline.
2. **Pact partner tenure scoring.** Weight long-standing pact partners higher in replacement decisions. If the attacker's Sybils are all recent pacts (< 3 months), clients could flag the lack of tenure diversity.
3. **WoT cluster diversity enforcement.** Make cluster diversity a protocol-level constraint, not just a client-side preference. Require that no more than 30% of active pacts come from the same WoT cluster (defined by shared follow overlap).
4. **Client-side Sybil detection heuristics.** Flag accounts that: (a) were created within the same time window, (b) follow the same set of targets, (c) have unusually similar posting patterns, (d) always respond to pact requests from the same user.

---

## Attack 2: Relay Cartel

### Objective

Five major relay operators collude. They control perhaps 60-80% of relay traffic (reflecting real-world power-law relay usage). What can they learn, censor, and manipulate?

### Step-by-Step Attack Plan

#### Phase 1: Intelligence Gathering

1. **Social graph reconstruction.** Each relay sees which pubkeys publish to it, which clients connect and fetch events, and the timing of all operations. By merging connection logs across 5 relays, the cartel can reconstruct a substantial portion of the social graph. Specifically:
   - Follow relationships: if pubkey A consistently fetches events authored by pubkeys B, C, D from relay R1, and pubkeys E, F from relay R2, the cartel infers A follows B, C, D, E, F.
   - Posting patterns: timestamps, frequency, volume per pubkey.
   - IP-to-pubkey mapping: every connecting client reveals its IP. With 5 relays, the cartel has high confidence in IP assignments.

2. **Pact topology inference.** NIP-46 encrypted blobs pass through relays for pact communication. The cartel cannot read the content (NIP-44 encrypted), but can observe:
   - Which pairs of pubkeys exchange NIP-46 blobs.
   - Timing correlation: pubkey A stores a blob at 14:03:01, pubkey B fetches at 14:03:04. Repeated for the same pair = probable pact relationship.
   - Across 5 relays, the cartel can map a significant fraction of the pact topology.

3. **Pseudonymous request deanonymization.** Kind 10057 data requests use pseudonymous pubkeys: `bp = H(target_root_pubkey || YYYY-MM-DD)`. The cartel can pre-compute these hashes for every known pubkey (they have the full list from step 1). When a pseudonymous request arrives, they look up which pubkey it targets. **The pseudonymous scheme provides zero protection against any relay that knows the target pubkey.** The daily rotation prevents cross-day linking by passive observers, but relay operators who know the pubkey set can trivially reverse the hash for every request they see.

#### Phase 2: Censorship

4. **Selective event dropping.** The cartel silently drops events from targeted pubkeys. During the Bootstrap and Hybrid phases (0-15 pacts), the user depends heavily on relays. Dropping their events effectively silences them.

5. **Pact disruption.** The cartel identifies pact-related NIP-46 traffic (from timing analysis) and selectively delays or drops it. This disrupts:
   - Challenge-response exchanges (kind 10054): delayed responses cause reliability scores to degrade.
   - Pact formation negotiation (kinds 10055, 10056, 10053): delayed or dropped offers prevent new pacts from forming.
   - Checkpoint distribution (kind 10051): delayed checkpoints prevent light nodes from syncing.

6. **Challenge-response sabotage.** The cartel can selectively delay NIP-46 blobs carrying challenge responses. If a challenge response arrives after the response window (1h for hash challenges between full nodes, 4-8h for active nodes, 24h for intermittent), the challenged peer fails the challenge. Repeated failures drop the reliability score below 50%, triggering pact drops. **The cartel can systematically destroy pact relationships by introducing 500ms-2s of jitter on challenge-response traffic.**

#### Phase 3: Manipulation

7. **Selective delay for information asymmetry.** The cartel delays delivery of events to specific users while allowing others to see them immediately. This creates an information asymmetry exploitable for social engineering, market manipulation, or political influence.

8. **Withholding kind 10050 updates.** If a user revokes a compromised device by publishing a new kind 10050, the cartel can delay propagation of this revocation. The compromised device continues to be accepted by peers who haven't seen the update.

### Cost Estimate

- **Time:** Ongoing operational cost.
- **Money:** The 5 relay operators are already running infrastructure. Additional cost is engineering time to build the analysis and censorship tooling -- approximately $50,000-100,000 one-time, plus ongoing maintenance.
- **Expertise:** High. Requires traffic analysis, timing correlation, and coordination across 5 organizations.

### Protocol Defenses

1. **Outbox model fragmentation.** Users publish to multiple relays. The cartel with 5 relays might miss traffic going through the other 995 relays. But if those 5 relays handle 60-80% of traffic, fragmentation is limited.
2. **Pact partner storage.** Sovereign users (15+ pacts) have their data stored by 20 pact partners. Relay censorship cannot delete this data. But relay censorship *can* prevent new users from discovering it.
3. **NIP-44 encryption.** Content is encrypted. The cartel cannot read DMs or pact negotiation content. But metadata (who talks to whom, when, how often) is exposed.
4. **Pseudonymous requests.** Offer no protection against the cartel. As noted above, any relay that knows the pubkey set can trivially reverse the daily hash.

### Verdict

| Dimension | Rating |
|-----------|--------|
| Exploitability | **Moderate** (requires 5 operators to collude, but relay concentration makes this plausible) |
| Impact | **Critical** |

The relay cartel is the protocol's most dangerous adversary class. The pseudonymous request scheme is ineffective against relay operators -- it was designed against passive network observers, not the relays themselves. The cartel can reconstruct social graphs, infer pact topology, deanonymize read patterns, selectively censor, and sabotage pact challenge-response. The only saving grace is that Sovereign users (15+ pacts) can survive relay censorship. But the cartel can prevent users from *reaching* Sovereign phase by disrupting pact formation during Bootstrap.

### Suggested Mitigations

1. **Strengthen pseudonymous requests.** The current `H(pubkey || date)` scheme is trivially reversible by any party that knows the pubkey. Consider a scheme where the requester generates a random blinding factor and the storage peer uses a private matching protocol (e.g., PSI-style) that doesn't reveal the target to the relay.
2. **Out-of-band challenge-response.** For Full-Full pact pairs, consider allowing challenge-response to travel via direct connections (NAT-hole-punched) rather than relay-mediated channels, removing the relay from the critical path.
3. **Pact formation fallback.** If relay-mediated pact negotiation fails repeatedly, fall back to direct peer-to-peer negotiation via FIPS or BLE mesh, bypassing relays entirely.

---

## Attack 3: Targeted Deanonymization

### Objective

Given a target pubkey, determine the real IP address and physical identity of the person behind it, using the cheapest available methods.

### Attack Path 1: Relay Logs (Cost: Low)

1. **Query the target's relay list.** Kind 10002 (NIP-65) lists the user's preferred relays. These are public.
2. **Obtain relay logs.** Legal process (subpoena), bribery of relay operators, or operating one's own relay that the target uses. The relay log reveals the IP address of every client that connected.
3. **Correlate timing.** The target publishes event at timestamp T. The relay log shows which IP connected and published at T. With multiple timestamps, the correlation becomes high-confidence.
4. **IP to identity.** ISP records (via legal process or data broker), WHOIS for static IPs, or geolocation databases.

**Cost:** $0 if you operate a relay the target uses. $500-5,000 for legal process against a relay operator. $50,000+ if bribing relay operators in hostile jurisdictions.
**Time:** Days to weeks.

### Attack Path 2: NAT Hole-Punch Signaling (Cost: Low-Medium)

1. **Become a pact partner.** Follow the target, get followed back (social engineering), wait 7 days, form a pact.
2. **Initiate direct connection.** As a pact partner, request bulk sync via direct connection. The NAT hole-punch signaling reveals the target's IP address.
3. **Alternative:** Even without becoming a pact partner, if you operate a relay that handles hole-punch signaling for the target, you see the IP addresses exchanged in the signaling messages.

**Cost:** Weeks of social engineering + relay operation costs.
**Time:** 2-4 weeks.

### Attack Path 3: Timing Analysis (Cost: Medium)

1. **Monitor publish timestamps.** The target's events have `created_at` timestamps. Aggregate over weeks to build an activity pattern (time zone, sleep schedule, work hours).
2. **Correlate with public information.** If the target's posting pattern matches a known timezone + sleep pattern + work schedule, this narrows the candidate pool.
3. **Device fingerprinting.** Kind 10050 reveals device types (mobile, desktop, extension, server) and uptime patterns. A user with a desktop at 91% uptime and a mobile at 28% uptime reveals: always-on desktop (likely home or office), phone used intermittently. The uptime patterns reveal lifestyle information.

**Cost:** Scripting time only.
**Time:** 2-4 weeks of data collection.

### Attack Path 4: BLE Scanning (Cost: Low, but requires physical proximity)

1. **Physical presence.** Be in the same physical location as the target.
2. **Scan for BLE advertisements.** If the target has nearby mode enabled, detect BLE mesh traffic.
3. **Correlate.** The target's ephemeral key is not linked to their root pubkey -- but if the target activates nearby mode and posts a public event within seconds of each other, the timing correlates the ephemeral key to the root pubkey.
4. **Physical identification.** You are physically nearby. BLE range is ~100m. Combined with visual observation, you can identify the person.

**Cost:** Physical travel to the target's location (derived from timezone analysis).
**Time:** Hours once location is known.

### Combined Attack

The cheapest path to full deanonymization:

1. Analyze posting timestamps (free, days of passive observation) -- confirm timezone and daily schedule.
2. Operate a relay or subpoena relay logs ($0-5,000) -- get IP address.
3. IP to identity via ISP or data broker ($100-1,000) -- get real name and address.

**Total cost: $100-6,000. Total time: 1-3 weeks.**

### Protocol Defenses

1. **Outbox model.** Prevents direct IP exposure between social contacts. But relays see IPs.
2. **Pseudonymous requests.** Prevent observers from seeing read patterns. But relay operators can reverse the hash (see Attack 2).
3. **Ephemeral keys for BLE.** Prevent identity linkage in nearby mode. But timing correlation can bridge the gap.
4. **NIP-44/NIP-59 encryption.** Content is protected. Device metadata (types, uptime) is now encrypted in kind 10050. But timing metadata remains public.

### Verdict

| Dimension | Rating |
|-----------|--------|
| Exploitability | **Easy** (relay log path) to **Moderate** (full physical identification) |
| Impact | **Critical** (complete identity exposure) |

The protocol does not protect against relay-side deanonymization. Any relay operator can link pubkeys to IPs. The outbox model distributes exposure but does not eliminate it.

### Suggested Mitigations

1. **Tor/VPN enforcement option.** Clients should offer an option to route all relay connections through Tor. This is the only defense against relay-side IP logging.
2. **Timestamp fuzzing.** Add configurable jitter (0-60 seconds) to `created_at` timestamps to degrade timezone and activity pattern analysis.

---

## Attack 4: Data Hostage Attack

### Objective

Form pacts with a target, reliably store their data, then threaten to withhold it unless paid. Test the 20-peer redundancy against a colluding majority.

### Step-by-Step Attack Plan

1. **Build 11 identities.** Over 3-4 months, build 11 Sybil identities within the target's WoT (see Attack 1 methodology). Each identity is a full node (Keeper) with 95%+ uptime to be attractive as a pact partner.

2. **Occupy pact slots.** Form pacts with the target. Store their data honestly. Pass all challenges with 100% reliability scores. The goal is to become the target's most trusted pact partners.

3. **Wait for dominance.** Over months, some of the target's legitimate pact partners will churn (go offline, fail challenges, leave the network). The attacker's Sybils, with their perfect reliability scores, are never dropped. Natural churn shifts the ratio in the attacker's favor.

4. **Coordinate the hostage demand.** All 11 Sybils simultaneously:
   - Stop responding to data requests (kind 10057/10058) for the target's data.
   - Continue responding to challenge-response (kind 10054) to maintain pact status.
   - Deliver the ransom demand via out-of-band channel.

5. **Leverage.** The target has 20 active pact partners. 11 are hostile. The remaining 9 still hold the data. But:
   - If any of those 9 are light nodes (30% uptime), the data may not be available during the 70% downtime.
   - The attacker can time the demand to coincide with periods when several legitimate partners are offline.
   - The attacker can further degrade the target's position by having Sybils that are 2-hop WoT peers refuse to forward gossip for the target.

### Why 11-of-20 is Dangerous but Not Fatal

The critical insight: challenge-response and data serving are separate operations. The protocol challenges pact partners to prove they *store* data, but challenge success does not guarantee they will *serve* it to gossip requesters.

A pact partner can:
- Pass hash challenges (proves possession).
- Pass serve challenges (proves local storage).
- Refuse to respond to kind 10057 pseudonymous requests (no protocol penalty for this -- serving gossip requests is voluntary).

The attacker stores the data and passes all challenges, maintaining a 90%+ reliability score, while simultaneously refusing to serve the data to anyone who asks for it via gossip.

**However:** The 9 remaining honest pact partners can still serve the data. With even 2-3 full nodes among the honest 9, the data remains available (uptime probability: `1 - (0.05)^2 * (0.70)^7 = 1 - 0.0025 * 0.0824 = 99.98%`). The hostage attack creates *degraded* availability, not *zero* availability.

The attacker's real leverage is psychological and social, not technical. "I have your data and I *could* leak it" is more threatening than "I'm withholding service."

### Cost Estimate

- **Time:** 4-6 months to establish 11 Sybil identities with pact slots.
- **Money:** Infrastructure for 11 always-on full nodes: approximately $55/month (11 x $5 VPS). Total over 6 months: $330.
- **Expertise:** Moderate social engineering + basic infrastructure management.

### Protocol Defenses

1. **20-peer redundancy.** The attacker needs a majority (11+) which is expensive to achieve.
2. **Challenge-response.** Detects non-storage, but the attacker *is* storing the data.
3. **WoT cluster diversity.** If enforced, prevents all 11 Sybils from the same social cluster. But the attacker can distribute Sybils across clusters.
4. **Standby pact promotion.** 3 standby pacts provide additional honest partners -- the attacker needs 12+ including standbys.

### Verdict

| Dimension | Rating |
|-----------|--------|
| Exploitability | **Hard** (requires 4-6 months and 11+ Sybil identities in the target's WoT) |
| Impact | **Medium** (degraded availability, not data loss; real leverage is threat of data leakage, not withholding) |

The data hostage attack is expensive to execute and does not achieve total data unavailability. The attacker's best leverage is the threat of *leaking* stored data (especially DMs), not withholding it. But DMs are NIP-44 encrypted to the recipient's DM key -- the attacker (as a pact partner storing NIP-59 gift wraps) holds opaque encrypted blobs, not plaintext DMs.

### Suggested Mitigations

1. **Gossip serving as challenge component.** Track whether pact partners respond to gossip requests for the user's data. A partner that passes hash challenges but never serves gossip requests is suspicious -- add a "gossip responsiveness" dimension to reliability scoring.
2. **Pact tenure diversity alerts.** If more than 50% of a user's pact partners are newer than 3 months, alert the user. This catches the Sybil buildup phase.
3. **Encrypted pact data.** Consider encrypting events before storing them with pact partners (encrypt to the user's own key). This eliminates the data leakage threat -- pact partners hold encrypted blobs they cannot read. Cost: pact partners cannot serve the data to third-party requesters without the user's involvement (breaks the cascading read-cache model).

---

## Attack 5: Guardian Abuse

All items addressed. Guardian-incentives.md design doc covers reputation tracking, multi-guardian support, seedling-initiated replacement, and anti-gaming mechanisms.

---

## Attack 6: Timing Attack on DM Recipients

### Objective

Without decrypting NIP-44 encrypted DMs, determine who is messaging whom by analyzing delivery timing patterns through relays.

### Step-by-Step Attack Plan

1. **Relay operator position.** Operate a popular relay (or collude with one). Observe all NIP-59 gift-wrapped DM deliveries.

2. **DM delivery pattern analysis.** A DM from Alice to Bob follows this path:
   - Alice encrypts with NIP-44 to Bob's `dm_key`.
   - Alice wraps in NIP-59 sealed gift addressed to Bob's root pubkey.
   - Alice's device publishes to relay.
   - Bob's device fetches from relay.

   The relay sees:
   - Alice's device pubkey publishes a kind 1059 (gift wrap) at time T1.
   - Bob's device pubkey fetches it at time T2.
   - The gift wrap's `p` tag contains Bob's root pubkey (NIP-59 requires this for relay routing).

   **Wait -- NIP-59 gift wraps expose the recipient's pubkey in the `p` tag.** The relay knows exactly who the recipient is. The sender's device pubkey is also visible (it signs the outer event). The relay can link device pubkey to root pubkey via kind 10050.

3. **Conversation reconstruction.** Over days-weeks, the relay observes:
   - Alice's device publishes gift wraps tagged for Bob: T1, T3, T7, T15.
   - Bob's device publishes gift wraps tagged for Alice: T2, T5, T9.
   - Pattern: alternating messages with response times of 1-5 minutes = active conversation.

4. **Frequency analysis.** Without decrypting content:
   - Daily message count between any two pubkeys.
   - Average response time (indicates relationship closeness).
   - Active hours (timezone and schedule).
   - Conversation initiation patterns (who messages first).

5. **Social graph inference from DMs.** DM metadata reveals the *private* social graph -- relationships that are not reflected in public follow lists. This is often more sensitive than the public graph.

### Cost Estimate

- **Time:** Passive observation. Zero additional effort beyond relay operation.
- **Money:** Standard relay operation costs.
- **Expertise:** Basic data analysis.

### Protocol Defenses

1. **NIP-44 encryption.** Content is encrypted. The relay cannot read messages.
2. **NIP-59 gift wrapping.** Provides metadata privacy by hiding the sender. **But the recipient's pubkey is in the `p` tag** (required for relay routing). And the sender's device pubkey signs the outer event, which is linkable to the root pubkey via kind 10050.
3. **DM key separation.** DMs encrypt to the `dm_key`, not the root key. This is irrelevant to timing analysis.

### The Critical Gap

NIP-59 gift wraps expose the recipient in the `p` tag. The sender is identifiable via the signing device pubkey + kind 10050 resolution. **The relay knows both sender and recipient of every DM.** NIP-59's metadata privacy is limited to hiding the sender from the *recipient's* relay (if different), but the *sender's* relay sees everything.

If Alice and Bob use the same relay, that relay sees both sides of every DM exchange.

### Verdict

| Dimension | Rating |
|-----------|--------|
| Exploitability | **Easy** (passive observation by any relay operator) |
| Impact | **High** (complete DM relationship graph; private social graph exposure) |

DM timing analysis is trivial for relay operators. The protocol provides no meaningful protection against DM metadata analysis. Content is encrypted, but the communication graph is fully exposed.

### Suggested Mitigations

1. **Decoy DM traffic.** Clients periodically publish empty gift wraps to random pubkeys, creating noise in the timing analysis. Cost: bandwidth + relay storage. Effectiveness: depends on decoy volume relative to real DM volume.
2. **DM relay rotation.** Use different relays for DM delivery to different recipients. No single relay sees the complete DM graph. This mirrors the outbox model fragmentation for regular events.
3. **Batch DM delivery.** Instead of publishing DMs immediately, queue them and publish in batches at fixed intervals (e.g., every 15 minutes). This degrades timing precision.
4. **Anonymous DM relaying.** Use onion-routed delivery (per ADR 008 suggestion 12) for DMs specifically. Each hop strips a layer of encryption, hiding the sender from the delivery relay.
5. **Remove recipient from p tag.** This is a fundamental NIP-59 design issue. If the relay cannot route by recipient pubkey, it must store all gift wraps and let recipients filter client-side. This is expensive but eliminates recipient metadata. Alternatively, use a blinded recipient identifier similar to kind 10057's `bp` tag.

---

## Attack 7: Device Compromise Cascade

### Objective

Compromise one device subkey. Determine the blast radius: can the attacker pivot to other devices, the root key, or the DM key?

### Scenario: Compromised Mobile Device (with DM capability)

The attacker compromises the user's phone. The phone holds:
- Device subkey (private)
- DM key (private, if `dm` flagged)
- Possibly cached events and pact partner information

### What the Attacker Can Do

1. **Post as the user.** Sign kind 1, 6, 7, 9, 14, 30023 events with the device subkey. These are attributed to the root identity via `root_identity` tag. The attacker can:
   - Publish defamatory, illegal, or reputation-damaging content.
   - React to and repost content to manipulate social relationships.
   - Publish events that consume the user's pact partners' storage.

2. **Read all DMs (current rotation epoch).** The DM key decrypts all DMs encrypted to the current `dm_key`. This exposes up to ~37-97 days of DM history (30-90 day configurable rotation + 7-day grace). The attacker can read the user's private conversations.

3. **Send DMs as the user.** The attacker can compose and send NIP-44 encrypted DMs, impersonating the user in private conversations.

4. **Read cached data.** Any events, pact partner information, or relay credentials cached on the device are accessible.

5. **Respond to storage challenges.** If the device is a checkpoint delegate, the attacker can publish checkpoints. If the device manages pact storage, the attacker can respond to (or deliberately fail) storage challenges.

### What the Attacker Cannot Do

1. **Revoke or add devices.** Kind 10050 requires the root key (cold storage). The attacker cannot publish a new device delegation.

2. **Undo their own revocation.** When the user discovers the compromise and revokes the device (publishing a new kind 10050 from cold storage), the attacker cannot re-add themselves. **This is the key improvement over standard Nostr's shared-key model.**

3. **Update profile or follow list.** Kind 0 and kind 3 require the governance key. Unless the compromised device also holds the governance key (only trusted devices should), the attacker cannot change the user's profile or follows.

4. **Access the root key.** Device subkeys are independent (not derived from root). Compromising a device key reveals nothing about the root key.

5. **Access other device keys.** Device subkeys are independent per-device. Compromising one device reveals nothing about other devices' keys.

6. **Read future DMs (after rotation).** After the next DM key rotation (at most 30-90 days, configurable), the compromised DM key expires. The attacker loses DM access.

### Pivot Opportunities

Can the attacker escalate from a compromised device key to broader access?

1. **Root key extraction: No.** Root key is in cold storage (hardware wallet, air-gapped machine). Not on the compromised device.

2. **Governance key extraction: Maybe.** If the compromised device is "trusted" and holds the governance key, the attacker gets profile/follow control. **The protocol's defense depends on users correctly restricting the governance key to truly trusted devices.** A user who puts the governance key on their phone (for convenience) undermines this defense.

3. **Other device key extraction: No.** Device keys are independent. No derivation path exists.

4. **DM key to root key: No.** The DM key is derived from root via KDF, but the KDF is one-way. Knowing `KDF(root, "dm-decryption-" || epoch)` does not reveal `root`.

5. **Pact partner compromise: Limited.** The attacker can see pact partner identities (kind 10053 stored on device) and could attempt to sabotage those relationships. But they cannot forge events signed by pact partners' keys.

6. **Social recovery exploitation: Maybe.** If the device caches kind 10060 events (recovery delegation), the attacker learns the recovery contact set. However, the attacker could use the compromised device to *impersonate* the user to recovery contacts, initiating a social recovery attack with higher credibility.

### Blast Radius Summary

| Asset | Compromised? | Duration |
|-------|-------------|----------|
| Day-to-day posting | Yes | Until device revoked |
| DM reading (current epoch) | Yes (if dm-flagged) | Until DM key rotation (30-90 days, configurable) |
| DM sending | Yes (if dm-flagged) | Until device revoked |
| Profile/follows | No (unless governance key on device) | -- |
| Root key | No | -- |
| Other device keys | No | -- |
| Future DMs (post-rotation) | No | -- |
| Pact relationships | Observable, not forgeable | Until device revoked |

### Cost Estimate (for the attacker to compromise a device)

- **Physical theft:** Cost of physically stealing the phone. Varies wildly.
- **Remote exploit:** $50,000-500,000 for a zero-day mobile exploit (state-sponsored tier). $5,000-50,000 for known vulnerability chains (criminal tier).
- **Malware/phishing:** $500-5,000 for commodity mobile malware. Success rate varies.

### Verdict

| Dimension | Rating |
|-----------|--------|
| Exploitability | **Hard** (requires device compromise, which is outside the protocol's control) |
| Impact | **High** (DM exposure + impersonation capability, but bounded by rotation and revocation) |

The key hierarchy design works as intended. Device compromise is contained to the device's capability set. The root key in cold storage prevents escalation to full identity takeover. DM key rotation provides temporal bounding. The main risk is the 30-90 day window of DM exposure (configurable) and the impersonation capability until revocation.

### Suggested Mitigations

1. **Consider even faster DM key rotation for high-risk users.** The configurable 30-90 day rotation is a good improvement; consider allowing 7-day rotation for users who want tighter forward secrecy bounds. Cost: more frequent kind 10050 updates and DM key distribution.
2. **Device revocation propagation speed.** When a kind 10050 revocation is published, it must propagate quickly to all relays and peers. Consider a "revocation priority" mechanism that ensures revocation events are propagated faster than regular events.
3. **Client-side anomaly detection.** Clients should alert when events are signed by a device key from an unexpected IP, at unexpected times, or with unusual content patterns. This enables faster detection of compromise.
4. **DM key compartmentalization.** Consider deriving per-conversation DM keys rather than a single DM key for all conversations. A compromised device reveals DMs only for conversations actively cached on that device, not all conversations.

---

## Summary Matrix

| # | Attack | Exploitability | Impact | Worst Gap |
|---|--------|---------------|--------|-----------|
| 1 | Sybil WoT infiltration | Moderate | High | No minimum mutual-follow age for pacts; cluster diversity is client-side |
| 2 | Relay cartel | Moderate | **Critical** | Challenge-response transit sabotage |
| 3 | Targeted deanonymization | **Easy** | **Critical** | Relay logs expose IP |
| 4 | Data hostage | Hard | Medium | Challenge-response doesn't measure gossip serving willingness |
| 5 | Guardian abuse | -- | -- | Addressed by guardian-incentives.md |
| 6 | DM timing attack | **Easy** | High | NIP-59 gift wrap exposes both sender and recipient to relay |
| 7 | Device compromise cascade | Hard | High | 30-90 day DM exposure window |

## Prioritized Recommendations

These are the highest-leverage fixes based on the intersection of exploitability and impact.

### P0: Relay cartel challenge-response sabotage (Attack 2)

A relay cartel can selectively delay NIP-46 blobs carrying challenge responses, causing pact degradation that looks like network flakiness rather than an attack. This threatens pact stability for any user dependent on relay-mediated communication.

**Fix:** For Full-Full pact pairs, allow challenge-response via direct connections (NAT-hole-punched) rather than relay-mediated channels. Relay diversity requirements (min 3 relays, canary probing, collusion model) are now specified in relay-diversity.md.

### P1: DM metadata fully exposed to relay operators (Attack 6)

NIP-59 gift wraps expose both sender (via signing key) and recipient (via `p` tag) to relay operators. DM content is encrypted, but the communication graph is fully visible. This is now acknowledged in the whitepaper's "What We Don't Know Yet" section.

**Fix:** This requires changes to NIP-59 usage. Consider pseudonymous recipient identifiers, decoy traffic, or DM relay rotation as layered mitigations.

### P1: Sybil WoT infiltration enables eclipse attacks (Attack 1)

No minimum mutual-follow age for pact formation; WoT cluster diversity is a client-side heuristic, not a protocol constraint. An attacker with 2 months of patience can occupy a majority of a target's pact slots.

**Fix:** Require mutual-follow relationships to be at least 30 days old before pact formation. Enforce WoT cluster diversity at the protocol level, not just client-side.
