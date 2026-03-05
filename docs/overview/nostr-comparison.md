# Nostr Comparison

Gozzip inherits Nostr's primitives and works with Nostr infrastructure from day one. This isn't a fork or a replacement — it's a layer that adds data ownership, multi-device identity, and decentralized storage on top of what Nostr already provides. Every Nostr key, event, and relay works unchanged.

## What We Keep from Nostr

The entire existing Nostr event ecosystem works unchanged:

- **Event structure** — signed JSON objects with kind, pubkey, content, tags, sig
- **Kind 0** — user profiles (name, about, picture)
- **Kind 1** — short notes, replies, threads (NIP-10 conventions)
- **Kind 3** — follow lists (contact list)
- **Kind 6** — reposts
- **Kind 7** — reactions
- **Kind 14** — direct messages (NIP-44 encryption, NIP-59 sealed gifts)
- **Kind 30023** — long-form articles (NIP-23, parameterized replaceable)
- **Kind 9 / 39000-39009** — group chats and group metadata (NIP-29)
- **Kind 9734 / 9735** — zap requests and receipts (NIP-57)
- **Key format** — secp256k1 keypairs, same as Nostr
- **Relay protocol** — WebSocket subscriptions, filters

## What We Add

New primitives on top of Nostr:

| Primitive | Kind | Purpose |
|-----------|------|---------|
| Device delegation | 10050 | Map device subkeys to a root identity |
| Checkpoint | 10051 | Sync state for light nodes |
| Conversation state | 10052 | DM read-state sync across devices |
| `root_identity` tag | — | Attribute device-signed events to root identity |
| Client-side device resolution | — | Client resolves device → root via Kind 10050 (relay oracle optional) |
| Storage pact | 10053 | Reciprocal storage commitment between WoT peers |
| Storage challenge | 10054 | Proof of storage via challenge-response |
| Pact request/offer | 10055/10056 | DVM-style discovery of storage partners |
| Data request/offer | 10057/10058 | DVM-style retrieval from storage peers |
| Storage endpoint hint | 10059 | Encrypted peer endpoints for gossip discovery |
| Recovery delegation | 10060 | Designate N-of-M social recovery contacts |
| Recovery attestation | 10061 | Attest root key rotation (7-day timelock) |
| Per-event hash chain | — | seq + prev_hash tags for stream completeness |
| Onion routing (relay service) | — | Optional privacy layer for gossip routing, paid via Lightning |

See [Protocol > Messages](../protocol/messages.md) for full event schemas.

## What We Change

- **Authorship model** — events are signed by device subkeys, not the root key directly. The `root_identity` tag links them back. This enables multi-device without sharing private keys.
- **Follow semantics** — following someone is a commitment to index their network. You keep your follows wisely because you bear the cost.
- **Node sovereignty** — users are not fully dependent on relays. Light nodes use checkpoints to sync efficiently. Full nodes maintain complete history.
- **Storage model** — data is stored by reciprocal WoT peers, not just relays. Relays become optional accelerators.
- **Key hierarchy** — root key moves to cold storage. Derived DM and governance keys serve specific purposes on devices. Device keys remain independent.
- **Incentive model** — storage reliability and relay curation translate to content reach through pact-aware gossip routing. Lightning zaps provide an optional premium layer for relay services. No external subscriptions or tokens needed.
- **Offline transport** — BLE mesh via [bitchat](https://github.com/permissionlesstech/bitchat) interop enables event exchange without internet. Nearby discovery uses ephemeral subkeys for privacy.

## Migration Strategy

- Existing Nostr keys work as Gozzip root keys — no new identity needed
- Existing Nostr events are valid Gozzip events (no `root_identity` tag means root key signed directly, which is the Nostr default)
- Clients add device delegation and `root_identity` tagging incrementally
- Standard Nostr relays work without changes — clients handle all Gozzip-specific resolution. Optimized relays can optionally add oracle resolution for faster queries.
- Follow lists (kind 3) are identical — no migration needed

## Cross-Protocol Interoperability

Gozzip is not locked to Nostr. The protocol speaks Nostr natively because Nostr has the best primitives for censorship-resistant identity, but the data belongs to users and can move between protocols.

**Why this works:** Gozzip events are self-authenticating signed JSON. They can be verified, converted, and served regardless of the transport protocol. The storage layer (pacts, gossip, challenges) is independent of the event format — it stores and serves signed blobs.

| Protocol | Bridge approach | What maps |
|----------|----------------|-----------|
| **ActivityPub** (Mastodon, Pleroma, Misskey) | Event ↔ Activity conversion | Kind 1 → Create/Note, Kind 3 → Follow, Kind 7 → Like, Kind 6 → Announce |
| **AT Protocol** (Bluesky) | Event ↔ Record conversion | Kind 1 → app.bsky.feed.post, profiles → app.bsky.actor.profile |
| **RSS/Atom** | Export-only | Feed of Kind 1/30023 events as syndication entries |
| **Matrix** | DM bridge | Kind 14 ↔ Matrix encrypted messages |

**Identity bridging:** A Gozzip user's secp256k1 pubkey can be linked to an ActivityPub actor URI or an AT Protocol DID via a signed attestation (the user signs a statement binding the identities). This allows cross-protocol follows and content attribution.

**Data export:** A user can export their complete event history as signed JSON (self-authenticating, verifiable by anyone). This export can be:
- Imported into any other Gozzip client (keys verify)
- Converted to ActivityPub activities for Mastodon import
- Converted to AT Protocol records for Bluesky import
- Published as a static archive (signed, tamper-evident)

The protocol is a data ownership layer first. Nostr is the native transport. Everything else is a bridge.
