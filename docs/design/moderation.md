# Content Moderation and Reporting
**Date:** 2026-03-14
**Status:** Draft

## Overview

Gozzip inherits Nostr's censorship-resistant architecture: there are no global administrators, no central content policy, and no mandatory filtering. This is a feature. It is also a problem. Without any moderation primitives, the protocol becomes hostile to the people it claims to serve -- they are exposed to spam, harassment, illegal content, and coordinated abuse with no tools to respond.

This document specifies the moderation primitives Gozzip provides. The protocol does not enforce content policy. It provides tools that users, clients, relay operators, and third-party services use to implement their own policies. The distinction matters: the protocol is infrastructure, not governance.

The existing moderation framework (`moderation-framework.md`) defines the foundational event kinds (kind 10063 for deletion requests, kind 10064 for content reports, NIP-32 labels). This document extends that foundation with detailed designs for content reporting, labeling integration, blocklist federation, CSAM handling, coordinated harassment defense, moderation services, and pact-level moderation.

## 1. Content Reporting

### Design

Content reports use kind 10064 (defined in the moderation framework). This section specifies the full reporting flow, privacy model, and relay operator integration.

### Report Event (Kind 10064)

Extends NIP-56 reporting with Gozzip-specific semantics:

```json
{
  "kind": 10064,
  "pubkey": "<reporter_device_pubkey>",
  "tags": [
    ["root_identity", "<reporter_root_pubkey>"],
    ["e", "<reported_event_id>"],
    ["p", "<reported_author_root_pubkey>"],
    ["report", "<reason>"],
    ["detail", "<free-text explanation>"],
    ["relay", "<relay_url_where_content_was_seen>"],
    ["protocol_version", "1"]
  ],
  "content": "<NIP-44 encrypted payload to relay operator pubkey>",
  "sig": "<signed by reporter device key>"
}
```

**Reason tags:**

| Reason | Description | Expected response |
|--------|-------------|-------------------|
| `spam` | Unsolicited commercial content, repetitive noise | Relay operator discretion |
| `harassment` | Targeted abuse, threats, doxxing | Relay operator review |
| `impersonation` | Mimicking another user's identity | Relay operator review |
| `illegal_content` | Content illegal in the reporter's or relay's jurisdiction | Relay operator review, possible legal referral |
| `csam` | Child sexual abuse material | Mandatory: immediate removal, legal reporting (see Section 4) |
| `other` | Anything not covered above | Relay operator discretion |

### Privacy Model

Reports are NOT public by default. The `content` field is encrypted using NIP-44 to the relay operator's pubkey (published in the relay's NIP-11 information document). This means:

- **The reported user cannot see who reported them.** This prevents retaliation against reporters.
- **Other users cannot see the report.** This prevents weaponizing the report count as a public shame metric.
- **The relay operator can decrypt and read the report.** They are the intended recipient and decision-maker.
- **Optional: additional recipients.** If the reporter is in a NIP-29 community with designated moderators, the content can be multi-encrypted (NIP-44 to each moderator pubkey). The `content` field contains the encrypted payloads as a JSON array.

The unencrypted tags (`e`, `p`, `report`) allow relay-side indexing without decryption. A relay can count reports per pubkey without reading the details. The encrypted `content` provides context that requires deliberate operator action to access.

**Why not fully encrypted?** If the entire report were encrypted, relays could not index or aggregate reports without decryption. The compromise: report metadata (who reported whom, for what category) is visible to the relay; report details (free-text explanation, evidence) are encrypted to the operator.

### Report Routing

1. **Primary:** The report is published to the reporter's outbox relays (standard event publication).
2. **Targeted:** The report is also published to relays listed in the reported user's NIP-65 relay list, so the relay operators hosting the reported content receive the report.
3. **Optional moderation service:** If the reporter's client is configured with moderation service pubkeys, the content field is additionally encrypted to those pubkeys and published to their relay.

### Relay Operator Actions

The relay operator receives the report through their normal subscription flow. They can:

| Action | Mechanism | Protocol impact |
|--------|-----------|----------------|
| **Ignore** | No action taken | Report remains on record |
| **Remove event** | Relay deletes the reported event from its storage | Event may persist on other relays and pact partners |
| **Ban pubkey** | Relay adds pubkey to its deny list | Author can still publish to other relays |
| **Restrict pubkey** | Relay rate-limits or shadowbans the pubkey | Degraded service, not full ban |
| **Escalate** | Forward report to a moderation service or legal authority | See Sections 4 and 6 |

The protocol does not mandate any specific response. Relay operators are autonomous. A report is information, not a command.

### Report Abuse Prevention

Reports themselves can be weaponized (mass false reporting to silence a user). Defenses:

- **WoT weighting:** Reports from pubkeys within the relay operator's WoT carry more weight. Reports from unknown pubkeys are lower priority.
- **Rate limiting:** Kind 10064 events are subject to the same per-source rate limits as all events (10 req/s for pact-related, 50 req/s for data).
- **Reporter reputation:** A reporter who files many reports that operators consistently ignore accumulates a low implicit trust score. Clients and moderation services can track this.
- **No automatic action:** Reports never trigger automatic content removal. A human (relay operator or moderation service operator) always decides.

## 2. Labeling (NIP-32 Integration)

### Design

Labeling is the mechanism by which content is annotated with metadata that clients use for filtering. Gozzip uses NIP-32 labels without modification to the label event format, but specifies how labels interact with the WoT and how clients should present them.

### Label Events (Kind 1985)

Standard NIP-32 format:

```json
{
  "kind": 1985,
  "pubkey": "<labeler_pubkey>",
  "tags": [
    ["L", "<label_namespace>"],
    ["l", "<label_value>", "<label_namespace>"],
    ["e", "<labeled_event_id>"],
    ["p", "<labeled_pubkey>"]
  ],
  "content": "<optional justification>",
  "sig": "<labeler signature>"
}
```

### Standard Label Namespaces

| Namespace | Labels | Description |
|-----------|--------|-------------|
| `content-warning` | `nsfw`, `gore`, `spoiler`, `sensitive` | Content requiring user consent before display |
| `content-type` | `political`, `religious`, `commercial`, `satire` | Content categorization (no value judgment) |
| `trust` | `spam`, `bot`, `impersonation`, `scam` | Trust-relevant assessments |
| `legal` | `dmca`, `illegal`, `csam` | Legal status assessments (use with caution) |
| `quality` | `low-effort`, `ai-generated`, `repost` | Content quality signals |

Namespaces are conventions, not protocol-enforced. Clients recognize the namespaces they care about and ignore others. New namespaces emerge organically as communities need them.

### Trust Model for Labels

Not all labels are equal. A label from a random pubkey is noise. A label from a trusted source is signal. Clients determine trust through:

1. **Direct follow.** The user explicitly follows the labeler pubkey. Highest trust.
2. **WoT proximity.** The labeler is within 2 hops of the user's WoT. Moderate trust, weighted by hop distance.
3. **Moderation service subscription.** The user has explicitly subscribed to a moderation service (see Section 6) that publishes labels. High trust for that service's labels.
4. **Community endorsement.** A NIP-29 community the user belongs to has designated the labeler as a trusted source. Trust inherited from community membership.

**Labels from outside the user's trust radius are ignored by default.** A client can offer a setting to "show all labels" for users who want to see how strangers have labeled content, but the default is trust-filtered.

### Client-Side Content Filtering

Clients use labels to implement content filtering preferences:

```
User preference: hide content labeled "nsfw" from trusted labelers
```

The filtering logic:

1. Client fetches labels for events in the user's feed.
2. Client filters labels to those from trusted sources (per the trust model above).
3. Client applies the user's content preferences:
   - **Hide:** Content is not displayed. A placeholder indicates hidden content with the label reason.
   - **Blur:** Content is displayed behind a click-through warning (e.g., "This content has been labeled NSFW by @labeler_name").
   - **Show:** Content is displayed normally with a label badge.
4. User can override any label on a per-event basis ("show this anyway").

### What Labels Are Not

- **Not censorship.** Labels are metadata attached to content. The content itself is not modified, deleted, or prevented from propagating. Labels exist alongside the content, not instead of it.
- **Not mandates.** A label saying "spam" does not require any client to hide the content. It is information that clients can choose to act on.
- **Not consensus.** Two labeling services can label the same content differently. One might label a post "political," another might not. There is no resolution mechanism and none is needed -- users choose which labelers they trust.

## 3. Blocklist Federation

### Design

Users can publish and subscribe to blocklists -- lists of pubkeys they have chosen to block. Blocklists are opt-in, user-controlled, and federable.

### Blocklist Event (Kind 10070)

```json
{
  "kind": 10070,
  "pubkey": "<list_maintainer_root_pubkey>",
  "tags": [
    ["d", "<list_identifier>"],
    ["name", "<human-readable list name>"],
    ["description", "<what this list blocks and why>"],
    ["p", "<blocked_pubkey_1>"],
    ["p", "<blocked_pubkey_2>"],
    ["p", "<blocked_pubkey_3>"],
    ["updated_at", "<unix_timestamp>"]
  ],
  "content": "",
  "sig": "<signed by list maintainer>"
}
```

- **`d` tag:** Unique identifier for the list (allows the maintainer to publish multiple lists, e.g., "spam-accounts", "known-scammers", "csam-reporters").
- **`p` tags:** Each blocked pubkey. A list can contain any number of pubkeys.
- **Replaceable event:** Kind 10070 is a replaceable event (NIP-16 parameterized replaceable, keyed on `d` tag). Publishing a new version replaces the old one.

### Subscription Model

Users subscribe to a blocklist by following the list event:

```json
{
  "kind": 30078,
  "pubkey": "<subscriber_root_pubkey>",
  "tags": [
    ["d", "blocklist-subscriptions"],
    ["a", "10070:<list_maintainer_pubkey>:<list_identifier>"]
  ],
  "content": "",
  "sig": "<subscriber signature>"
}
```

Clients fetch subscribed blocklists on startup and merge the blocked pubkeys into the user's local block list. The merge is additive: the user's own blocks plus all subscribed list blocks.

### Community-Maintained Lists

Anyone can maintain a blocklist. Expected patterns:

- **Personal blocklists:** Individual users sharing their block list publicly. "Here are the accounts I've blocked -- subscribe if you trust my judgment."
- **Community blocklists:** A NIP-29 community maintainer publishing a list of pubkeys banned from the community. Other communities can subscribe to cross-reference.
- **Moderation service blocklists:** A moderation service (see Section 6) publishing blocklists based on their review process.

### Relay Operator Integration

Relay operators can subscribe to community blocklists for relay-level filtering:

- The relay fetches kind 10070 events from subscribed list maintainers.
- Blocked pubkeys are added to the relay's deny list.
- The relay refuses to store or serve events from denied pubkeys.

This is relay-operator-initiated. The protocol does not push blocklists to relays. Relay operators choose which lists to trust, just like individual users.

### No Default Blocklists

The protocol does not ship with any default blocklists. No client should pre-subscribe users to any blocklist without explicit opt-in. This is a firm design principle:

- Default blocklists create a de facto central authority (whoever maintains the default list).
- Default blocklists create a chilling effect (users may self-censor to avoid ending up on "the" list).
- Default blocklists are a single point of capture (if the default list maintainer is compromised or coerced, the entire network is affected).

Clients MAY suggest popular blocklists during onboarding ("Would you like to subscribe to any of these community blocklists?") but MUST NOT pre-activate them.

### Blocklist Transparency

Blocklists are public events. Anyone can inspect what pubkeys a list blocks and why. This transparency is intentional:

- Blocked users can see they are on a list and understand why (if the list description provides reasons).
- Researchers can audit blocklists for bias, overreach, or errors.
- Users can make informed decisions about which lists to trust.

Private blocklists (encrypted `p` tags) are possible but not specified here. A user who wants to block silently uses the existing NIP-51 mute list (kind 10000), which is encrypted.

## 4. CSAM Handling

### Legal Context

Child sexual abuse material (CSAM) is illegal in virtually every jurisdiction. Unlike other content moderation decisions (which are matters of policy), CSAM removal and reporting is a legal obligation. The protocol must provide clear, actionable mechanisms for participants to comply with the law.

This section is not about content policy. It is about legal compliance infrastructure.

### Relay-Level Hash Matching

Relay operators SHOULD implement perceptual hash matching against known CSAM hash databases:

- **PhotoDNA** (Microsoft, available to qualifying organizations via NCMEC)
- **CSAI Match** (Google, YouTube's matching system)
- **PDQ** (Meta, open-source perceptual hashing)

Implementation:

1. When a relay receives a media upload (via NIP-96 or equivalent), the relay computes the perceptual hash of the media.
2. The relay compares the hash against its local copy of the CSAM hash database.
3. On match: the relay rejects the upload, does NOT store the content, and logs the match for reporting.
4. The relay operator reports the match to the appropriate authority (NCMEC in the US, IWF in the UK, or the equivalent in their jurisdiction).

**This is not decentralized.** Hash databases are maintained by centralized organizations (NCMEC, IWF). Access requires application and vetting. This is a necessary compromise: CSAM detection requires centralized databases of known material, and there is no decentralized alternative that is legally or ethically acceptable.

**Relay operators who choose not to implement hash matching** are making a legal risk decision. The protocol does not mandate it, but relay operators in jurisdictions with mandatory reporting laws (e.g., the US under 18 U.S.C. 2258A) should understand their obligations.

### Pact-Level CSAM Response

If a pact partner's stored content is identified as CSAM (via report or hash match):

1. **Immediate content deletion.** The storing node deletes the specific content. No waiting period, no review, no appeal at the protocol level.
2. **Immediate pact dissolution.** The storing node drops the pact with the reported user. This pact drop does NOT count against the storing node's reliability score (same as kind 10064 `csam` reports, as specified in the moderation framework).
3. **Report generation.** The storing node publishes a kind 10064 report with `report: csam` to the reported user's relays and to any configured moderation services.
4. **No re-formation.** The storing node's client adds the reported pubkey to its local blocklist, preventing future pact formation.

**Why immediate and irreversible?** Because the legal obligation is unambiguous. In most jurisdictions, knowingly possessing CSAM is a criminal offense. The protocol provides no mechanism for "waiting to see if the report is valid" -- the storing node protects itself first.

**False positives exist.** Hash matching has a non-zero false positive rate. A legitimate user whose content hash-matches a known CSAM hash will experience pact dissolution and reporting. This is a known cost of the system. The user's recourse is:

- The reporting authority (NCMEC/IWF) reviews the material and determines it is not CSAM.
- The user is not prosecuted.
- The user can form new pacts (the blocklist is local to the reporting node, not network-wide).

This is imperfect. It is also the reality of every platform that implements CSAM scanning. The alternative -- not scanning -- creates legal liability for all participants.

### Emergency Reporting Path

For confirmed or high-confidence CSAM detection:

```
Detection (relay hash match or user report)
    |
    v
Relay operator reviews (if human review is configured)
    |
    v
Report to NCMEC CyberTipline (US) or IWF (UK)
    - Include: hash of material, IP metadata if available,
      reporter pubkey, reported pubkey, timestamp
    - Do NOT include: the material itself (hash reference only,
      unless law enforcement requests it)
    |
    v
Content removed from relay storage
Pubkey banned from relay (operator decision)
Kind 10064 report published (encrypted to operator pubkey)
```

**Relay operators SHOULD maintain a documented procedure** for CSAM reporting specific to their jurisdiction. The protocol provides the event kinds; the legal process is jurisdiction-dependent.

### Client-Side Defense in Depth

Clients SHOULD implement a perceptual hash check before displaying media:

1. Before rendering an image to screen, the client computes a perceptual hash (PDQ is open-source and suitable).
2. The client checks the hash against a locally bundled, regularly updated hash list (a subset of known CSAM hashes, distributed by organizations like NCMEC or the Tech Coalition).
3. On match: the image is NOT displayed. A placeholder is shown instead. The client optionally prompts the user to report.

**Why client-side?** Because relay-level scanning only catches content uploaded through that relay. Content arriving via pact partners, gossip, or direct messages bypasses relay scanning. Client-side checking is the last line of defense before content reaches a human eyeball.

**Performance:** PDQ hash computation is fast (~1ms per image on modern hardware). The hash database lookup is a hash table query. The overhead is negligible compared to image decoding and rendering.

**Privacy:** The hash check is entirely local. No image data or hash is sent to any external service. The hash database is downloaded in bulk and checked locally.

## 5. Coordinated Harassment Defense

### How WoT Limits Harassment

The 2-hop WoT boundary is the primary defense against harassment from strangers. Content from outside a user's WoT does not propagate to them through gossip. This means:

- A stranger cannot mention, quote, or reply-to a user and have that content reach the user through the gossip layer.
- A coordinated harassment campaign by 1,000 strangers produces zero gossip events in the target's feed.
- The target only sees harassment if they actively query relays for mentions (client-controllable).

**This is structurally different from open networks.** On Twitter, Mastodon, or Bluesky, any user can mention any other user and the mention is delivered. On Gozzip, mentions from outside the WoT are not delivered through gossip. They exist on relays, but they do not arrive unbidden.

### Within-WoT Harassment

Harassment from within the user's WoT (people they follow or friends-of-friends) is harder to prevent because the protocol is delivering content from trusted sources. Available responses:

| Action | Mechanism | Effect |
|--------|-----------|--------|
| **Mute** | NIP-51 kind 10000 (encrypted mute list) | Content from the muted pubkey is hidden in the user's client. The muted user does not know they are muted. |
| **Block** | Client-side blocklist | Content is hidden AND the user refuses to form pacts or serve data to the blocked pubkey. Stronger than mute. |
| **Report** | Kind 10064 report to relay operators | Relay operator may take action against the harasser. |
| **WoT edge removal** | Unfollow the harasser | Removes the harasser from the user's 1-hop WoT. If the harasser is also removed by mutual friends, they fall outside the 2-hop boundary entirely. |

### Compromised WoT Members

A specific scenario: a previously trusted WoT member begins harassing a user (account compromise, relationship breakdown, radicalization). The response sequence:

1. **Immediate: Mute.** Stops the content from appearing in the user's feed. Zero-latency defense.
2. **Report.** File kind 10064 with the appropriate reason (`harassment`). Relay operators and moderation services are notified.
3. **Optional: WoT edge removal.** Unfollow the harasser. This has cascading effects -- mutual friends who also unfollow the harasser push them further from the user's WoT.
4. **Optional: Pact dissolution.** If the user has an active pact with the harasser, dissolve it. The harasser loses access to the user's stored events through that pact.

The user does not need to do all of these. Muting alone may be sufficient. The escalation path exists for cases where it is not.

### Coordinated Inauthentic Behavior

A group of accounts acting in coordination to harass a target (brigading, dogpiling). WoT structure provides natural resistance:

- If the coordinated accounts are outside the target's WoT: no gossip delivery. The target is unaware unless they check relays.
- If some coordinated accounts are inside the WoT: the target sees those. Muting and reporting each account is the response.
- If the coordination is large enough to significantly overlap the target's WoT: this indicates a social crisis, not a protocol problem. The target's social graph is hostile to them. The protocol cannot fix this -- it is a human problem.

### What the Protocol Does NOT Do

- **No protocol-level throttling of mentions.** Throttling mentions would break legitimate use (e.g., a popular post generating many replies). The WoT boundary already limits who can reach the user.
- **No automatic harassment detection.** The protocol does not analyze content for harassment patterns. This is a client or moderation service responsibility.
- **No mandatory cooling-off periods.** Some platforms impose "you've been reported, you can't post for 24 hours" restrictions. Gozzip does not. Relay operators can implement this at their discretion, but it is not a protocol feature.

## 6. Moderation Services

### Design

Third-party moderation services operate as specialized Nostr identities (pubkeys) that subscribe to report feeds, analyze content, and publish moderation decisions as labels. Clients choose which moderation services to trust.

### How a Moderation Service Works

1. **Subscribe to reports.** The service subscribes to kind 10064 events from relays. If reporters have encrypted reports to the service's pubkey, the service can decrypt them.
2. **Analyze content.** The service reviews reported content -- manually, algorithmically, or both.
3. **Publish decisions as labels.** The service publishes NIP-32 label events (kind 1985) reflecting its assessment:

```json
{
  "kind": 1985,
  "pubkey": "<moderation_service_pubkey>",
  "tags": [
    ["L", "moderation"],
    ["l", "harassment-confirmed", "moderation"],
    ["e", "<reviewed_event_id>"],
    ["p", "<reviewed_author_pubkey>"]
  ],
  "content": "Reviewed report #<report_event_id>. Content constitutes targeted harassment per our community guidelines.",
  "sig": "<service signature>"
}
```

4. **Optionally publish blocklists.** The service can publish kind 10070 blocklists of pubkeys that have been repeatedly flagged.

### Client Integration

Clients that support moderation services:

1. Allow the user to add moderation service pubkeys (similar to adding a relay).
2. Fetch labels from subscribed moderation services.
3. Apply labels according to the user's content preferences (same filtering logic as Section 2).
4. Display moderation service attribution: "Flagged by @ModServiceName" rather than anonymous labels.

### Decentralized Moderation Model

There is no single moderation authority. The model is:

- **Multiple competing services.** Different moderation services can have different standards. A "strict" service might flag all NSFW content; a "permissive" service might only flag illegal content.
- **User choice.** Each user decides which services to subscribe to. No service is mandatory.
- **Transparency.** Moderation decisions are published as labels (public events). Anyone can audit a service's decisions.
- **Accountability.** A moderation service that makes poor decisions (too aggressive, too lenient, biased) loses subscribers. Market pressure.

### Comparison: Bluesky's Labeling Model

This design is directly inspired by Bluesky's labeling architecture (AT Protocol Lexicon `com.atproto.label`). Key parallels:

- Both use labels as the output of moderation (not content removal).
- Both allow multiple labeling/moderation services.
- Both let users choose which services to trust.

Key differences:

- Bluesky labels are tied to the AT Protocol's identity and hosting infrastructure. Gozzip labels are standard NIP-32 Nostr events, portable across any relay.
- Bluesky has a default moderation service (the Bluesky team's own labeler). Gozzip has no default -- see Section 3's "No Default Blocklists" principle.

## 7. Pact-Level Moderation

### The Carrier Question

When Alice's pact partner Bob stores Alice's events, and Alice's events include content that is reported or labeled as harmful, what is Bob's responsibility?

### Common Carrier Principle

By default, pact partners operate under a common carrier principle:

- **A pact partner stores data. They are not responsible for the content of that data.** This is the same principle that applies to ISPs, postal services, and cloud storage providers. The carrier transports packets; the sender is responsible for the content.
- **No obligation to review.** A pact partner is not required to inspect, filter, or moderate the content they store for a pact partner.
- **No liability for content.** A pact partner who stores harmful content on behalf of a pact partner is not the author or publisher of that content.

### Voluntary Moderation Actions

Despite the common carrier default, a pact partner MAY choose to moderate:

| Action | Mechanism | Protocol impact |
|--------|-----------|----------------|
| **Continue storing** | No action | Pact continues normally |
| **Dissolve pact** | Standard pact dissolution (kind 10053 with `status: dissolved`) | Partner stops storing the user's events. User's client initiates replacement pact. |
| **Flag to relay** | Publish kind 10064 report based on stored content | Relay operator receives report for review |
| **Selective deletion** | Delete specific flagged events while maintaining pact | Pact continues but reported events are removed from this partner's storage |

A pact partner who dissolves a pact due to content concerns is exercising their right to choose who they store data for. This is not censorship -- the user's events survive on other pact partners and relays. It is one node choosing not to participate.

### Reliability Score Impact

Pact dissolution for content reasons should not penalize the dissolving partner:

- If the dissolution accompanies a kind 10064 report with `csam` or `illegal` reason: no reliability score penalty (already specified in the moderation framework).
- If the dissolution accompanies a kind 10064 report with other reasons (`spam`, `harassment`, etc.): the client SHOULD treat this as a neutral dissolution, not a reliability failure. The dissolving partner is exercising judgment, not failing a storage obligation.
- If the dissolution has no accompanying report: standard reliability score penalty applies. This prevents abuse of "I didn't like the content" as a universal excuse for failing storage obligations.

### CSAM Exception

The common carrier principle has one hard exception: CSAM. As specified in Section 4:

- **CSAM storage is illegal in most jurisdictions regardless of carrier status.** The common carrier defense does not protect knowing possession of CSAM.
- **Pact partners who discover CSAM in their stored content MUST delete it immediately and SHOULD report it.**
- **This is the only case where the protocol specifies mandatory action.** All other moderation is voluntary.

## 8. Comparison with Existing Systems

### Nostr

Gozzip inherits Nostr's moderation primitives directly:

| Nostr NIP | Gozzip equivalent | Extension |
|-----------|--------------------|-----------|
| NIP-56 (Reporting) | Kind 10064 | Encrypted to relay operator; additional reason tags; WoT-weighted |
| NIP-32 (Labeling) | Kind 1985 (unchanged) | Trust model based on WoT proximity; moderation service integration |
| NIP-51 (Lists/Mutes) | Kind 10000 (unchanged) | Mute propagation hints within WoT |
| NIP-09 (Deletion) | Kind 10063 | Root/governance key signing; pact partner targeting |

**What Gozzip adds:** WoT-based trust weighting for reports and labels, encrypted report privacy, pact-level moderation semantics, blocklist federation.

### Mastodon (ActivityPub)

| Mastodon | Gozzip | Notes |
|----------|--------|-------|
| Server-level moderation by instance admins | No servers to moderate; relay operators fill a similar role | Mastodon admins have full control over their instance. Gozzip relay operators have full control over their relay. The difference is that Gozzip users are not bound to one relay. |
| Instance blocklists (defederation) | Blocklist federation (kind 10070) | Mastodon's instance blocks are all-or-nothing. Gozzip's blocklists are per-pubkey and opt-in. |
| Content warnings (CW) | NIP-32 labels with `content-warning` namespace | Functionally equivalent. Mastodon CWs are author-applied; Gozzip labels can be author-applied or third-party-applied. |
| Report to instance admin | Report to relay operator (kind 10064) | Similar flow. Gozzip reports are encrypted; Mastodon reports are visible to all instance admins. |

**Key difference:** Mastodon moderation is tied to the server. If your instance admin makes a moderation decision you disagree with, your options are limited (appeal, or move instances and lose your social graph). In Gozzip, moderation decisions by relay operators affect relay access, not identity or social graph. A user banned from one relay continues to exist on other relays and through pact partners.

### Bluesky (AT Protocol)

| Bluesky | Gozzip | Notes |
|---------|--------|-------|
| Labeling services (`com.atproto.label`) | Moderation services publishing NIP-32 labels | Closest model. Both use labels as moderation output, both allow multiple services, both let users choose. |
| Default labeler (Bluesky Moderation) | No default labeler | Bluesky ships with their own labeler pre-subscribed. Gozzip does not. This is a deliberate difference. |
| Account-level takedowns (PDS operator) | Relay-level bans (relay operator) | Bluesky's PDS hosts your data; a PDS takedown affects your ability to publish. Gozzip's relay bans affect one relay; pact partners provide independent storage. |
| Appeal process (Bluesky team) | No protocol-level appeal | Bluesky has a centralized appeal process. Gozzip has none -- appeals are between the user and the specific relay operator or moderation service. |

**Key difference:** Bluesky's architecture, despite its modular design, has practical centralization in the Bluesky PLC directory and the default labeler. Gozzip has no equivalent central points. This makes Gozzip harder to moderate globally (no kill switch for any account) and more resistant to capture (no single entity can define acceptable content for the network).

### Matrix

| Matrix | Gozzip | Notes |
|--------|--------|-------|
| Room-level moderation (room admins, power levels) | No rooms (yet); user-level moderation only | Matrix moderation is scoped to rooms. Gozzip has no room concept, so moderation is per-user and per-relay. |
| Mjolnir (moderation bot) | Moderation services (Section 6) | Similar concept: automated moderation tooling. Matrix's Mjolnir operates on rooms; Gozzip's moderation services operate on the open event stream. |
| Server ACLs | Relay deny lists | Matrix servers can block other servers. Gozzip relays can block pubkeys. |
| Trust & Safety team (for matrix.org) | No central T&S team | Same trade-off as Bluesky: centralized T&S is effective but creates a single point of control/failure. |

**Key difference:** Matrix's room model provides natural moderation boundaries (room admins control room policy). Gozzip's lack of rooms means moderation is either user-level (mute/block) or relay-level (operator decisions). If Gozzip adds rooms or group chats, room-level moderation similar to Matrix would be appropriate.

## Design Principles Summary

1. **The protocol provides tools, not rules.** Event kinds for reporting, labeling, and blocklists. No content policy.
2. **Privacy for reporters.** Reports are encrypted to the intended recipient. Reporters are protected from retaliation.
3. **No default authorities.** No pre-subscribed moderation services, no default blocklists, no mandatory labelers.
4. **WoT as the primary filter.** The 2-hop boundary prevents most unwanted content from arriving. Everything else is reinforcement.
5. **CSAM is the hard exception.** The only case where the protocol specifies mandatory action. Legal compliance is not optional.
6. **Decentralized moderation through market dynamics.** Multiple moderation services compete on quality. Users vote with subscriptions.
7. **Common carrier default for pact partners.** Storage is not endorsement. Except for CSAM, pact partners have no obligation to moderate stored content.
8. **Transparency.** Reports, labels, and blocklists are auditable. Moderation decisions are attributable to specific entities.

## Open Questions

- **Report encryption key rotation.** If a relay operator's key is compromised, all encrypted reports become readable. Should reports be encrypted with ephemeral keys? This adds complexity but limits exposure.
- **Cross-jurisdiction CSAM reporting.** A relay in Germany receives a CSAM report about content uploaded from Japan. Which reporting authority? The protocol does not specify -- this is a legal question, not a protocol question.
- **Moderation service discovery.** How do users find moderation services? Currently: word of mouth, client recommendations, community endorsements. A more structured discovery mechanism may be needed as the network grows.
- **Label spam.** A malicious labeling service could label all content as "spam" to degrade the labeling ecosystem. WoT-based trust filtering mitigates this, but the attack surface exists.
- **Blocklist size limits.** A blocklist with 100,000 pubkeys is a large event. Should kind 10070 support pagination or chunking? Practical concern for large-scale moderation services.
