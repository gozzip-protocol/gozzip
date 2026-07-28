# Content Moderation, Reporting, and Deletion

**Status:** Accepted

## Overview

Gozzip inherits Nostr's censorship-resistant architecture: no global administrators, no central content policy, no mandatory filtering. This is a feature, and it is also a problem. Without moderation primitives, the protocol becomes hostile to the people it claims to serve, exposing them to spam, harassment, and illegal content with no tools to respond.

This document specifies the moderation primitives Gozzip provides. The protocol does not enforce content policy. It provides tools that users, clients, and relay operators use to implement their own policies. The distinction matters: the protocol is infrastructure, not governance.

The canonical wire formats for the two custom moderation kinds — kind 10063 (deletion request) and kind 10064 (content report) — live in [`protocol/messages.md`](../protocol/messages.md). This document specifies how those kinds are used, not their byte layout. Where this doc and `messages.md` disagree on a field, `messages.md` wins.

## Design Principles

1. **The protocol provides tools, not rules.** Event kinds for reporting, labeling, and deletion. No content policy.
2. **No central authority.** Moderation is distributed across relay operators, labeling services, and individual users.
3. **Privacy for reporters.** Report detail is encrypted to the intended recipient so reporters are protected from retaliation.
4. **WoT as the primary filter.** The 2-hop web-of-trust boundary already prevents most unwanted content from arriving through gossip; everything else is reinforcement.
5. **Relay autonomy.** Relay operators set their own content policies, as they do in standard Nostr.
6. **CSAM is the one hard exception.** The only case where the protocol describes mandatory action, because the legal obligation is unambiguous.

## 1. Content Reporting (kind 10064)

Content reports let a user flag an event for a relay operator or community moderator. The event format — device-key signature, `root_identity` tag, `e`/`p` tags, category slug, and NIP-44-encrypted detail — is defined canonically in [`messages.md`](../protocol/messages.md). The reporting *model* is:

**Categories.** The `report` tag carries one category slug: `spam`, `harassment`, `impersonation`, `illegal_content`, `csam`, or `other`. Categories are routing hints for the recipient, not commands. Only `csam` carries a described mandatory response (Section 4).

**Report privacy.** The reporter signs with their device key and includes a `root_identity` tag so the recipient can resolve the reporter's identity through the kind 10050 delegation chain. The free-text explanation is NIP-44-encrypted to the relay operator's pubkey (published in the relay's NIP-11 document) rather than carried as a public `detail` tag. This means:

- The reported user cannot see who reported them, which prevents retaliation.
- Other users cannot see the report, which prevents weaponizing report counts as a public shame metric.
- The relay operator can decrypt and read the detail; they are the intended decision-maker.

The unencrypted tags (`e`, `p`, `report`) allow relay-side indexing and aggregation without decryption: a relay can count reports per pubkey without reading any detail.

**Routing.** A report is published to the reporter's own outbox relays and to the relays in the reported author's NIP-65 list, so operators hosting the content receive it. If the reporter's client is configured with community-moderator or third-party moderation-service pubkeys, the detail can be additionally encrypted to those pubkeys.

**Relay operator actions.** The operator receives the report through their normal subscription flow and decides what to do: ignore it, remove the event from their storage, ban or rate-limit the pubkey, or escalate to a legal authority. The protocol mandates none of these. A report is information, not an instruction.

**Report abuse.** Reports can themselves be weaponized through mass false reporting. Defenses are all recipient-side: reports from pubkeys inside the operator's WoT carry more weight; kind 10064 events are subject to the same rate limits as all events; a reporter whose reports are consistently ignored accumulates a low implicit trust score; and reports never trigger automatic removal — a human always decides.

## 2. Muting and Blocking (NIP-51)

Most user-level moderation needs no new event kinds. The existing WoT graph and NIP-51 lists cover them:

- **Mute** (NIP-51 kind 10000, an encrypted mute list) hides content from a pubkey in the user's own client. The muted user is not notified. This is the zero-latency first response to unwanted content from someone already inside the WoT.
- **Block** (client-side) is stronger: the client hides the content *and* refuses to form pacts with or serve data to the blocked pubkey.

**Mute propagation as a soft hint.** If several people a user follows all mute the same pubkey, the client MAY surface a suggestion ("3 people you follow have muted @spammer"). This is a UI hint computed from existing kind 10000 lists of followed users who opted into public mute sharing — not an automatic action and not a new event kind.

**Coordinated harassment.** The 2-hop WoT boundary is the structural defense: content from outside a user's WoT does not reach them through gossip, so a campaign by a thousand strangers produces zero events in the target's feed. Harassment from *inside* the WoT is handled by the escalation path — mute, then report, then unfollow (which, if mutual friends also unfollow, pushes the harasser past the 2-hop boundary), then dissolve any active pact. The user rarely needs all of these; muting alone is often enough.

## 3. Labeling (NIP-32)

Labeling annotates content with metadata that clients use for filtering. Gozzip uses NIP-32 label events (kind 1985) without modification, and specifies only how labels interact with the WoT.

**Namespaces** are conventions, not protocol-enforced: `content-warning` (`nsfw`, `gore`, `spoiler`), `content-type` (`political`, `commercial`, `satire`), `trust` (`spam`, `bot`, `scam`), `legal` (`dmca`, `csam`), `quality` (`low-effort`, `ai-generated`). Clients recognize the namespaces they care about and ignore the rest.

**Trust model.** Not all labels are equal. A label from a random pubkey is noise; a label from a trusted source is signal. Clients weight labels by a direct follow (highest), WoT proximity within 2 hops (weighted by hop distance), or an explicit subscription to a moderation service. Labels from outside the user's trust radius are ignored by default.

**Client filtering** applies the user's preference to trusted labels — hide (a placeholder shows the reason), blur (click-through warning), or show (label badge) — and the user can always override any label per event.

**What labels are not.** Labels are not censorship: the content is not modified or prevented from propagating; the label sits alongside it. Two services can label the same content differently, and no resolution mechanism is needed because users choose which labelers they trust.

**Ship a default label service, disclosed honestly.** The protocol ships no mandatory labeler and no default blocklist — a default authority would become a single point of capture. But a usable client needs *some* moderation on day one. The recommended posture: a client MAY ship with a default label service pre-configured, provided it is disclosed plainly in the UI ("filtered by X, which you can turn off or replace") and is trivially removable. Honesty about the default is the difference between a helpful starting point and a hidden gatekeeper.

**Shared block lists** use NIP-51. A user who wants a shared block list publishes an encrypted NIP-51 list — the existing mute-list machinery covers shared-list use without a dedicated blocklist kind.

## 4. CSAM: the relay-operator obligation

Child sexual abuse material is illegal in virtually every jurisdiction, and its removal and reporting is a legal obligation rather than a policy choice. This section is legal-compliance infrastructure, not content policy.

Relay operators SHOULD implement perceptual-hash matching (PhotoDNA, CSAI Match, or the open-source PDQ) against known CSAM hash databases maintained by NCMEC, IWF, or equivalents. On a match, the relay rejects and does not store the upload, and reports the match to the appropriate authority. This is deliberately *not* decentralized: CSAM detection requires centralized databases of known material, and there is no ethically acceptable decentralized alternative. Operators in jurisdictions with mandatory-reporting laws (for example 18 U.S.C. 2258A in the US) should understand their obligations; the protocol supplies the event kinds, the legal process is jurisdiction-dependent.

Because relay-level scanning only sees content uploaded through that relay, clients SHOULD also run a local PDQ check against a bundled hash list before rendering media, showing a placeholder on a match. The check is entirely local — no image or hash leaves the device.

**Pact partners** who discover CSAM in content they store MUST delete it immediately and SHOULD publish a kind 10064 report with `report: csam`. Dissolving a pact for this reason carries no reliability penalty (see below). This is the single case where the protocol describes mandatory action; all other moderation is voluntary.

**Common carrier, with one exception.** By default a pact partner is a common carrier: they store data and are not responsible for its content, with no obligation to inspect or moderate it. A partner MAY still choose to dissolve a pact or selectively delete a stored event over content concerns — that is their right to choose whom they store for, and the author's events survive on other partners and relays. A dissolution accompanied by a `csam` report carries no reliability penalty; a dissolution accompanied by any other report is treated as a neutral exit, not a storage failure; a dissolution with no report at all takes the normal reliability penalty, so "I didn't like the content" cannot become a universal excuse for failing an obligation. CSAM is the one hard exception to the common-carrier default, because the common-carrier defense does not protect knowing possession.

## 5. Deletion (kind 10063)

Authors can request deletion of their own events. Kind 10063 is Gozzip's protocol-level erasure mechanism and its answer to the GDPR Article 17 "right to erasure." The event format, the `reason` enum (`user_request`, `gdpr`, `content_violation`, `error`), and the optional `all_events` tag are defined canonically in [`messages.md`](../protocol/messages.md).

**Authorization.** A deletion request MUST be signed by the root key or governance key. Device keys cannot issue one, which prevents a compromised device from wiping an identity's history. The `pubkey` must match the author of the referenced events — a user cannot delete another user's events.

**Workflow.** The mechanism is intentionally simple:

1. The author publishes kind 10063, referencing specific events (`e` tags) or all of their events (`all_events`).
2. It propagates through the normal channels — the author's relays, and pact partners and read-caches via their existing subscriptions for the author.
3. Recipients who hold a referenced event delete it and retain the kind 10063 as a tombstone, which prevents re-ingestion from another source. Read-caches evict immediately, regardless of remaining TTL. Relays that support NIP-09 MAY treat kind 10063 as equivalent to a kind-5 deletion for their own retention.
4. **Enforcement is the ordinary pact mechanism, nothing new.** A pact partner that keeps serving a deleted event will, under normal data-availability scoring, produce checkpoint hashes that no longer match the author's expected state. That shows up as unreliability through the same challenge scoring that governs every pact ([ADR 014](../decisions/014-presence-aware-reliability.md)), and a partner that repeatedly ignores deletion requests is dropped and replaced like any other failing partner. Deletion compliance rides on the storage-reliability machinery that already exists.

**Honest limitation.** Deletion in a decentralized system is best-effort, and the protocol says so plainly. A signed event, once published, is independently verifiable by anyone holding a copy; a deletion request cannot unsign it. A partner can delete from its serving layer while retaining a shadow copy, and challenge scoring cannot prove otherwise. The protocol relies on the same incentive that makes storage work: a partner who behaves dishonestly risks losing the reciprocal storage they depend on. This is a reasonable-efforts standard, which is what GDPR asks of distributed systems — not a guarantee. Copies made by humans (screenshots, manual exports) and events on permanently-offline devices are outside any protocol's reach.

**Account deletion** is the root key's final act: publish a kind 10063 with `all_events`, dissolve all pacts, publish a final kind 10050 with an empty device list to revoke all delegation, and delete local data. Partners purge under the same scoring-enforced expectation as any other deletion.

This document specifies the mechanism and its honest limits, not legal analysis. A Data Protection Impact Assessment belongs to counsel before any EU launch.
