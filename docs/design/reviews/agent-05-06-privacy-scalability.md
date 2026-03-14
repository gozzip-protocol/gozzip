# Adversarial Review: Privacy & Scalability

**Agents:** 05 (Privacy) + 06 (Scalability)
**Date:** 2026-03-14
**Scope:** Whitepaper v1.2, surveillance-surface.md, media-layer.md, relay-diversity.md, gdpr-deletion.md, messages.md
**Methodology:** Fresh adversarial analysis. Every issue described below is derived from the source material listed above.

---

## 1. Executive Summary

Gozzip makes an honest effort to delineate what it hides and what it leaks, and the existing surveillance-surface document is unusually candid for a protocol in this space. However, several gaps remain between the claimed privacy properties and what the protocol actually delivers. The pseudonymous request token (H(pubkey||date)) provides negligible privacy against any relay that maintains a pubkey directory -- which is every relay. NIP-59 DM metadata leakage fully exposes the communication graph to relay operators. BLE physical-layer fingerprinting undermines the cryptographic unlinkability that ephemeral subkeys are supposed to provide. On the scalability side, media pacts introduce order-of-magnitude storage asymmetries that the volume-matching tolerance cannot absorb, and the GDPR deletion mechanism has structural gaps for BLE-replicated and re-ingested content. The relay cartel threat is acknowledged but insufficiently mitigated: 3-5 popular relays can reconstruct near-complete social graphs, pact topologies, and DM communication graphs with no protocol-level countermeasure beyond "use more relays."

---

## 2. Issues

### P-01: Rotating Request Tokens Provide No Meaningful Privacy Against Relays

**Severity:** High

**Description:** The rotating request token `H(target_pubkey || YYYY-MM-DD)` is presented as providing "pseudonymous lookup" and protecting against "casual observers." In practice, every relay that stores events for any set of users has a directory of pubkeys. Computing today's token for every known pubkey is trivial -- a relay with 100,000 known pubkeys computes all tokens in under a second. This means every relay can resolve every data request (kind 10057) to its target identity in real time. The token provides zero privacy against relays.

The whitepaper and surveillance-surface document both acknowledge this ("trivially reversible by any party that knows the target's public key"), but the protocol still presents the token as a privacy mechanism in Section 5.2 and uses language like "pseudonymous" that implies protection. The day-boundary linkage (matching both today's and yesterday's token) further weakens even the nominal cross-day unlinkability.

**Recommendation:** Either (a) replace the rotating token with a proper cryptographic blinding scheme (e.g., blind signatures from the target, PIR-style queries, or oblivious PRF), or (b) reclassify the token explicitly as a "relay optimization hint" with no privacy properties and remove all privacy-related language from its description. Option (b) is honest and costs nothing. Option (a) is complex but would provide real privacy.

---

### P-02: NIP-59 Gift Wraps Expose Full DM Communication Graph

**Severity:** High

**Description:** The surveillance-surface document states clearly: "NIP-59 gift wraps expose both sender and recipient to the relay operator. The sender is identifiable via the signing device pubkey (resolvable to root identity through kind 10050). The recipient's pubkey appears in the `p` tag, required for relay routing." This means every relay handling DM traffic sees both parties, timing, frequency, and response patterns of every DM exchange it processes.

This is not a minor metadata leak -- it is a complete communication graph exposure. For users under surveillance (journalists, activists, dissidents), this is functionally equivalent to unencrypted metadata. A single relay operator, without any collusion, can build a complete DM interaction graph for all users who route DMs through that relay.

The mitigation options listed ("DM relay rotation," "batch DM delivery," "pseudonymous recipient identifiers") are all marked "not yet implemented." The protocol ships with full DM metadata exposure and no countermeasure.

**Recommendation:** Implement at minimum one of the listed mitigations before claiming any DM privacy beyond content encryption. DM relay rotation is the lowest-cost option: use a different relay for DMs with each recipient, ensuring no single relay sees the full communication graph. Pseudonymous recipient identifiers (requiring recipients to poll rather than relay-route) would provide stronger protection but at usability cost. This should be a pre-launch requirement, not a future consideration.

---

### P-03: Kind 10050 Device Metadata Creates a Device Fleet Fingerprint

**Severity:** Medium

**Description:** The kind 10050 event lists all device pubkeys in public `device` tags. While device type, uptime, and DM capability are encrypted in the content field (per messages.md), the number of devices and their public keys are visible to any observer. A user with 3 device subkeys is distinguishable from one with 1 device subkey. Over time, the addition and removal of device tags creates a temporal fingerprint. Combined with the `root_identity` tag on every event (which links device subkeys to root identity), a relay can track which devices are active at which times.

The messages.md notes: "Relays can link the two devices to one identity (this is by design)." This is acknowledged as a trade-off, but the privacy cost is understated. The relay does not just learn "these devices belong to one person" -- it learns device count, device lifecycle (when devices are added/removed), and per-device activity patterns.

**Recommendation:** Consider using a single ephemeral wrapper key for relay-facing operations, with device-specific keys only visible to pact partners (who need them for challenge routing). This would hide the device count and lifecycle from relays while preserving the pact partner routing functionality. Alternatively, accept this as a deliberate trade-off and document the full extent of the exposure (device count, lifecycle, per-device activity patterns) rather than just "these two devices belong to the same person."

---

### P-04: NIP-46 Timing Correlation Reveals Pact Topology

**Severity:** Medium

**Description:** The whitepaper and relay-diversity document both acknowledge that NIP-46 store/fetch timing patterns can reveal pact pairings to relay operators. The surveillance-surface document states: "a persistent relay operator observing for 2-4 weeks can probabilistically identify pact pairs despite jitter." The relay-diversity document confirms this with "Medium-High" confidence for merged NIP-46 timing analysis across colluding relays.

The pact topology is the core infrastructure of the protocol. If a relay cartel can map pact relationships, they can identify whose data each user stores, which users depend on which full nodes, and which relationships are most critical to network connectivity. Combined with the DM communication graph (P-02) and social graph from subscription patterns, this gives a comprehensive view of the entire protocol's infrastructure layer.

The specified mitigations are: relay diversity (min 3 relays, max 40% concentration per relay), relay rotation every 30 days, and optional cover traffic. Cover traffic is the only measure that addresses timing correlation directly, and it is opt-in with a bandwidth cost.

**Recommendation:** Make cover traffic (dummy NIP-46 operations) the default, not opt-in. The bandwidth cost is small relative to the pact communication traffic itself. Quantify the actual bandwidth overhead of cover traffic at sufficient levels to defeat timing correlation (e.g., doubling the NIP-46 message rate with dummy messages). If the overhead is acceptable, make it default. If not, at minimum make it default for privacy mode and document the residual risk for standard mode.

---

### P-05: BLE RF Fingerprinting Defeats Cryptographic Unlinkability

**Severity:** Medium

**Description:** The surveillance-surface document acknowledges this clearly: "Every BLE radio has manufacturing imperfections -- clock drift, frequency offset, power amplifier nonlinearities -- that create a unique radio signature." Ephemeral subkey rotation provides cryptographic unlinkability, but the physical layer provides persistent hardware-level linkability.

This is correctly identified as a fundamental limitation of BLE hardware, not a protocol design flaw. However, the practical implication is more severe than the document suggests. The document frames this as requiring "SDR equipment," but commodity BLE sniffers (nRF52840 dongles, ~$10) combined with open-source fingerprinting software can achieve >90% accuracy. In environments where BLE mesh is most valuable (protests, censored environments), adversaries are most likely to deploy such equipment.

Additionally, the `root_identity` tags in relayed mesh events create a parallel identity channel: even if the relay device uses ephemeral keys, the events it relays carry root identity metadata that a BLE scanner can observe. This is noted in the document ("Mesh relay linkability via root_identity tags") but deserves escalation.

**Recommendation:** For the relay linkability issue: strip `root_identity` tags from events during BLE mesh relay, replacing them with opaque forwarding headers that only the intended recipient can resolve. This requires the relay hop to re-wrap events, which conflicts with the self-authenticating property (recipients need the original signature), so a compromise is needed -- perhaps encrypt the `root_identity` tag to the event's intended destination during BLE relay. For RF fingerprinting: the document's recommendation ("disable BLE in adversarial environments") is the correct advice but should be surfaced prominently in user-facing guidance, not buried in a design document.

---

### P-06: Permanent Non-Repudiable Signatures Enable Retroactive Deanonymization

**Severity:** Medium

**Description:** The surveillance-surface document correctly identifies this: "If a pseudonymous pubkey is ever linked to a real-world identity... ALL historical events signed by that pubkey are attributed." Every event is permanently, irrevocably linked to the author's pubkey. Combined with the lack of forward secrecy for public content, this creates a retroactive deanonymization risk where a single operational security failure exposes the entire posting history.

This is an inherent property of Nostr-compatible signed events and cannot be changed without breaking the self-authenticating property. However, the protocol could mitigate the blast radius.

**Recommendation:** Consider supporting ephemeral posting identities: a user can create a short-lived pubkey, link it to their root identity via an encrypted (not public) attestation, and post under the ephemeral identity. If the ephemeral pubkey is compromised, only that subset of posts is attributed. The attestation link is only visible to the user and designated recovery contacts. This does not solve the fundamental problem but limits the blast radius. Alternatively, document this as a hard constraint and explicitly recommend that users who need pseudonymous posting use separate, unlinked pubkeys with no cross-referencing.

---

### S-01: Media Pact Storage Asymmetry Exceeds Volume Tolerance

**Severity:** Medium

**Description:** The media-layer document specifies +/-100% volume tolerance for media pacts (vs. +/-30% for text pacts). This means a user producing 50MB/month can be paired with one producing 100MB/month -- a 2x asymmetry. But the document's own calculations show that media volume is "bursty" (a vacation week produces 200MB; the next month, zero). A user who averages 50MB/month but spikes to 250MB during a vacation breaks the 100% tolerance against a partner averaging 50MB/month.

The 30-day trailing volume calculation smooths some of this, but a single burst month can push the ratio well beyond 2x. With 90-day retention, a single burst month persists in the storage obligation for 3 months.

Additionally, media pact count is specified as 3-10 (Keepers only). If only 5-25% of network participants are Keepers, and media pacts are restricted to Keepers, the pool of eligible media pact partners is small. Volume matching within this small pool further constrains options. A user needing 5 media pact partners from a pool of (0.15 * 20 =) ~3 Keepers in their WoT may find it impossible to form sufficient media pacts.

**Recommendation:** (a) Use a 60-day or 90-day trailing volume for media matching (not 30-day) to smooth bursts. (b) Consider allowing media pact partners outside the immediate 2-hop WoT boundary, since media pacts are less sensitive than text pacts (media blobs are content-addressed and self-verifying, so trust requirements are lower). (c) Model the Keeper availability constraint explicitly: given the expected Keeper ratio, how many media-volume-compatible Keepers exist within 2-hop WoT for a typical user?

---

### S-02: Full-Node Outbound Bandwidth is Dominated by Text Pact Serving

**Severity:** Medium

**Description:** The media-layer document's bandwidth calculations show that full-node daily outbound is ~300MB regardless of media intensity, dominated by "Serve text pact data." This number is suspiciously high. The document specifies 20 pact partners, each producing 30 events/day at 925 bytes = 27.75KB/day/partner. Even serving all 20 partners' complete daily output (27.75KB * 20 = 555KB) plus responding to read requests does not approach 300MB/day.

The 300MB figure likely includes serving read-cache data to gossip requesters (Tier 3 reads), which would mean the full node is serving far more than just its pact partners' data. This transforms full nodes from bilateral pact participants into de facto content distribution nodes -- bearing CDN-like outbound bandwidth for popular content they happen to cache.

This is a scalability concern: full nodes with popular followed authors in their WoT will bear disproportionate outbound bandwidth serving cached reads. The read-cache serving is opt-in (default false for media), but the text read-cache appears to be always-on.

**Recommendation:** Clarify the 300MB/day full-node outbound figure -- break it down into pact serving vs. read-cache serving vs. gossip forwarding. If read-cache serving dominates, provide explicit controls (rate limits, bandwidth caps) for read-cache outbound. Consider making text read-cache serving opt-in for full nodes that are bandwidth-constrained, or implementing a bandwidth budget with proportional fairness across requesters.

---

### S-03: GDPR Deletion Has Structural Gaps for BLE and Re-Ingestion Scenarios

**Severity:** Medium

**Description:** The gdpr-deletion document is thorough and honest about limitations. However, several structural gaps remain:

**BLE copies:** Events replicated via BLE mesh to offline devices persist indefinitely until the device comes online. The document acknowledges this ("BLE mesh copies may persist") but the 7-hop relay limit does not bound the number of devices that may hold copies. In a mesh with 100 devices, a popular event could be on dozens of devices, each of which must independently come online and process the deletion request.

**Re-ingestion via bridge services:** The whitepaper states that public content can be exported to ActivityPub, AT Protocol, and RSS/Atom via bridge services. A deletion request (kind 10063) has no meaning in these foreign protocols. Content exported to Mastodon, Bluesky, or an RSS feed persists there regardless of the Gozzip deletion request. The gdpr-deletion document does not address cross-protocol deletion.

**Tombstone retention:** The document raises tombstone retention as an open question but does not resolve it. Without indefinite tombstone retention, deleted events can be re-ingested from pact partners, relays, or caches that did not process the deletion request in time. A 1-year tombstone with re-publication is proposed but not specified -- this is a compliance gap.

**Repost deletion:** The document notes that a repost (kind 6) by another user is not covered by the original author's deletion request. This is correct but means that content reported as CSAM or illegal via kind 10064 cannot be fully purged by the original author's deletion request alone -- each reposter must independently delete.

**Recommendation:** (a) Specify mandatory indefinite tombstone retention for kind 10063 events -- the storage cost is negligible (one small event per deletion). (b) Address cross-protocol deletion: bridge services that export Gozzip content to foreign protocols SHOULD subscribe to kind 10063 events for bridged authors and propagate deletions to the foreign protocol. (c) For CSAM/illegal reports: specify that kind 10063 + kind 10064 together should trigger deletion of reposts by pact partners who hold them, not just the original event. (d) Acknowledge the BLE gap quantitatively: estimate the expected number of BLE copies and the expected time for all to process the deletion request.

---

### S-04: Relay Cartel Threat Is Insufficiently Mitigated

**Severity:** High

**Description:** The relay-diversity document states the threat clearly: 3-5 popular relays colluding can reconstruct ~80% of the social graph, identify probable pact pairings (Medium-High confidence), reverse all rotating request tokens for known pubkeys (High confidence), and see the full DM communication graph (High confidence). Content remains encrypted.

The mitigation hierarchy is: (1) relay diversity, (2) NIP-59 gift wrapping for NIP-46, (3) Tor for relay connections, (4) cover traffic. Items 2-4 are marked as not yet implemented or opt-in.

The protocol currently ships with only relay diversity as a defense, and the document itself states: "Even with perfect relay diversity: a colluding majority still reconstructs most of the graph from merged partial views." This means the protocol's only implemented defense is acknowledged as insufficient.

The real-world relay ecosystem is highly concentrated. Nostr currently has ~1,000 relays, but traffic is dominated by a handful. If Gozzip inherits this concentration, the relay cartel threat is not hypothetical -- it is the default deployment scenario.

**Recommendation:** (a) Prioritize implementing NIP-59 gift wrapping for NIP-46 pact communication -- this is the highest-impact mitigation that does not require Tor. (b) Implement cover traffic as default, not opt-in. (c) Consider a relay reputation system where clients track which relays serve events reliably and which drop or delay events selectively, sharing these observations through the WoT (not globally, to prevent gaming). (d) Explore a mixed-relay strategy where each protocol operation type (event publishing, DMs, pact communication, data requests) uses a different relay, maximizing the number of relays an adversary must compromise for full visibility.

---

### S-05: Equilibrium-Seeking Formation Produces Asymmetric Keeper Burden

**Severity:** Medium

**Description:** The whitepaper describes an asymmetric equilibrium: Keepers reach comfort at ~7 pacts, Witnesses at ~35. The PACT_FLOOR of 12 forces Keepers to accept beyond their comfort level, but the ceiling is 40. A Keeper at the floor of 12 may be approached by many Witnesses seeking high-uptime partners. Without rate limiting on pact formation requests, popular Keepers (those with high uptime and good WoT connectivity) could be inundated with pact requests.

The functional diversity constraint (at least 15% of active pacts must be Keepers) means that in a network with 5% Keepers, each Keeper must serve ~3x its share of pacts. At 5% Keepers in a 5,000-node network (250 Keepers), each Keeper would need to hold pacts with ~60 Witnesses on average (assuming 20 pacts per Witness with 15% Keeper requirement = 3 Keeper pacts per Witness = 14,250 Keeper-pact slots / 250 Keepers = 57 per Keeper). This exceeds the 40-pact ceiling.

**Recommendation:** Model the Keeper demand explicitly under different Keeper ratios (5%, 10%, 15%, 25%). Identify the minimum Keeper ratio at which the 40-pact ceiling is not binding. If the minimum viable Keeper ratio exceeds what is realistically achievable (comparable systems achieve 0.1-5%), the protocol needs either: (a) a mechanism to incentivize Keeper participation beyond social reciprocity, (b) relaxation of the 15% functional diversity constraint, or (c) introduction of a "relay-as-Keeper" model where paid relays serve as high-uptime pact partners.

---

### S-06: Gossip Fan-Out Creates Distinguishable Traffic Patterns for ISPs

**Severity:** Low

**Description:** The surveillance-surface document identifies that gossip fan-out (one inbound event followed by multiple outbound events to different relay connections) creates a traffic pattern that distinguishes Gozzip from vanilla Nostr. Challenge-response messages at regular intervals add further distinction.

This is correctly identified as an ISP-level traffic analysis concern, and the document recommends Tor/VPN for users in hostile network environments. However, the document does not quantify the distinguishability. How many events per hour would an ISP need to observe to classify a connection as "Gozzip, not vanilla Nostr" with >95% confidence? If the answer is "a few minutes of observation," the threat is more immediate than the document suggests.

**Recommendation:** Quantify the distinguishability window -- how much traffic does an ISP need to observe to fingerprint Gozzip vs. vanilla Nostr with high confidence? If it is short (minutes), consider implementing basic traffic padding (fixed-rate messages on relay connections) as a default, not just a future consideration. The bandwidth cost of low-rate padding is negligible for always-on connections.

---

### S-07: Media Encryption for DMs Is Unspecified

**Severity:** Medium

**Description:** The media-layer document's Open Question #3 states: "DM media should be encrypted to the recipient before hashing. The hash then references the ciphertext, and only the recipient can decrypt. Encryption scheme: NIP-44 symmetric encryption with the media blob as payload."

This is listed as an open question, not a specification. Until this is resolved, DM media blobs referenced via `media` tags are stored unencrypted on CDNs, pact partners, or IPFS -- anyone with the SHA-256 hash can fetch and view them. Since the hash appears in the event's `media` tag, which is inside the NIP-44-encrypted DM content, only the sender and recipient know the hash. But the media blob itself sits unencrypted on whatever hosting service the sender used.

If the hosting service logs access patterns (as CDNs typically do), the CDN operator can see who uploaded the media and potentially infer that it is DM-related (not publicly referenced). Worse, if the CDN or any intermediary has the hash, they can fetch and view the media -- it is not encrypted at rest.

**Recommendation:** Promote this from "open question" to "required specification." DM media MUST be encrypted to the recipient before upload. The `media` tag hash references the ciphertext. The encryption scheme (NIP-44 symmetric encryption with a per-media random key, key conveyed in the encrypted DM content) should be specified and mandatory for any media referenced in a kind 14 DM event.

---

### P-07: Kind 10059 Storage Endpoint Hints Leak Pact Topology to Followers

**Severity:** Low

**Description:** Kind 10059 events are sent to followers within 2-hop WoT and contain encrypted peer endpoints. While the content is encrypted, the existence of the event and its `p` tag (follower pubkey) are visible to the relay. When the event is updated (pact partners change), the relay sees the update timing. Over time, the frequency and timing of kind 10059 updates correlate with pact churn.

More importantly, the recipient (follower) decrypts the peer endpoints and learns which storage peers hold the author's data. This is by design (the follower needs this for Tier 2 cached endpoint retrieval), but it means every 2-hop WoT follower learns the author's current pact partner endpoints. A malicious follower within 2-hop WoT now knows where to target for data availability attacks.

**Recommendation:** Consider tiered endpoint disclosure: share full endpoint lists only with pact partners; share a subset (2-3 endpoints) with general WoT followers. This limits the attack surface while preserving Tier 2 functionality for most reads.

---

### P-08: Recovery Delegation Pseudonymous Identifiers Are Linkable

**Severity:** Low

**Description:** Kind 10060 uses `H(recovery_contact_pubkey || owner_pubkey)` as the `d` tag. This hides the recovery contact's identity from third parties who do not know either pubkey. However, a relay that knows the owner's pubkey (trivially, since the event's `pubkey` field is the root pubkey) can enumerate candidate recovery contacts by computing `H(candidate || owner_pubkey)` for all known pubkeys and matching against the `d` tag. The search space is the relay's pubkey directory -- the same brute-force approach that defeats the rotating request token (P-01).

**Recommendation:** Use a blinded identifier that requires knowledge of a secret not available to the relay -- e.g., `H(recovery_contact_pubkey || owner_pubkey || shared_secret)` where the shared secret is exchanged out-of-band between the owner and the recovery contact. Alternatively, encrypt the entire kind 10060 event to the recovery contact and store it as an opaque blob (the relay does not need to index recovery delegations by `d` tag if the recovery contact polls for events addressed to them).

---

## 3. Summary Table

| ID | Title | Severity | Category | Status |
|----|-------|----------|----------|--------|
| P-01 | Rotating request tokens provide no meaningful privacy against relays | High | Privacy | Acknowledged but insufficiently mitigated |
| P-02 | NIP-59 gift wraps expose full DM communication graph | High | Privacy | Acknowledged, no mitigation implemented |
| P-03 | Kind 10050 device metadata creates device fleet fingerprint | Medium | Privacy | Partially acknowledged |
| P-04 | NIP-46 timing correlation reveals pact topology | Medium | Privacy | Acknowledged, cover traffic opt-in only |
| P-05 | BLE RF fingerprinting defeats cryptographic unlinkability | Medium | Privacy | Acknowledged as hardware limitation |
| P-06 | Permanent non-repudiable signatures enable retroactive deanonymization | Medium | Privacy | Acknowledged, no mitigation |
| P-07 | Kind 10059 endpoint hints leak pact topology to followers | Low | Privacy | By design, risk understated |
| P-08 | Recovery delegation pseudonymous identifiers are linkable | Low | Privacy | Not acknowledged |
| S-01 | Media pact storage asymmetry exceeds volume tolerance | Medium | Scalability | Not addressed |
| S-02 | Full-node outbound bandwidth dominated by read-cache serving | Medium | Scalability | Not addressed |
| S-03 | GDPR deletion has structural gaps for BLE and re-ingestion | Medium | Compliance | Partially acknowledged |
| S-04 | Relay cartel threat insufficiently mitigated | High | Privacy/Scalability | Acknowledged, mitigations not implemented |
| S-05 | Equilibrium-seeking formation produces asymmetric Keeper burden | Medium | Scalability | Partially addressed (PACT_FLOOR) |
| S-06 | Gossip fan-out creates distinguishable traffic patterns | Low | Privacy | Acknowledged |
| S-07 | Media encryption for DMs unspecified | Medium | Privacy | Listed as open question |

---

## 4. Overall Assessment

The protocol documentation is unusually honest about its privacy limitations. The surveillance-surface document, the relay-diversity document, and the whitepaper's "What We Don't Know Yet" section collectively acknowledge most of the issues raised here. This intellectual honesty is a strength -- it means the design team understands the threat model.

However, there is a gap between acknowledgment and mitigation. The three highest-severity issues (P-01, P-02, S-04) are all acknowledged in the documentation but have no implemented countermeasures. The protocol ships with rotating tokens that provide no real privacy, DM metadata fully exposed to relays, and no defense against relay collusion beyond relay diversity -- which the documentation itself says is insufficient.

**Privacy grade: C+.** The protocol provides meaningful privacy improvements over centralized platforms (no single point of full visibility, encrypted content, relay fragmentation). But against a moderately motivated adversary (a relay operator with a pubkey directory), the metadata exposure is comprehensive: social graph (~80% reconstructible), DM communication graph (fully visible), pact topology (probabilistically identifiable), and data request targets (fully resolvable). The privacy model is "better than Facebook, worse than Signal" -- which is a legitimate design choice, but the documentation should state this comparison explicitly rather than using language like "pseudonymous" that implies stronger guarantees.

**Scalability grade: B-.** The core architecture scales well for text-only workloads. Media separation is well-designed. The equilibrium-seeking pact formation is mathematically sound. But the Keeper burden asymmetry may be unsustainable at low Keeper ratios, and the media pact constraint (Keepers only, within 2-hop WoT) may be too restrictive for media-heavy users. The 300MB/day full-node outbound needs clarification -- if read-cache serving is the dominant component, full nodes are bearing CDN-like costs without CDN-like incentives.

**Pre-launch blockers:** P-02 (DM metadata) and S-07 (DM media encryption) should be resolved before any deployment targeting privacy-sensitive users. S-03 (GDPR tombstone retention) should be specified before EU deployment. The remainder are design improvements that can be addressed iteratively.
