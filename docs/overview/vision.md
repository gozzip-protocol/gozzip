# Vision

Gozzip is an open, censorship-resistant protocol for social media and messaging. It inherits the proven primitives of Nostr — secp256k1 identity, signed events, relay transport — and adds a layer where users truly own their data: stored by their social graph, portable across protocols, and independent of any single platform or server.

It should feel like Twitter from the user's perspective — fast, simple, no friction — while being sovereign, encrypted, and decentralized underneath.

## UX Principle

The user never thinks about:

- Which device signed an event
- How device keys relate to their identity
- How their feed is being indexed
- Where their data is stored
- How encryption works

Everything just works: multi-device, direct messages, encrypted content, public feeds, fast timelines, good UI. The technical complexity is real but entirely hidden. If a feature requires the user to understand cryptography, the design is wrong.

## Sovereignty Without Burden

Users own their identity and their social graph. But sovereignty doesn't mean work. The protocol handles:

- Multi-device sync automatically (fork-and-reconcile)
- Follow indexing in the background
- Key management transparently
- Light/full node switching based on device capability
- Data storage via reciprocal WoT pacts (no relay dependency)
- Incentives are emergent — reliable storage and good curation earn wider reach, no tokens or subscriptions. See [ADR 009](../decisions/009-incentive-model.md).

The user opens the app and sees their feed. That's it.

## Community Parallel

The protocol's design mirrors human social dynamics. Each protocol role maps to an observable pattern in how communities form, maintain relationships, and propagate information.

| Persona | Protocol Role | Human Equivalent |
|---------|--------------|------------------|
| **Keeper** | Full node pact partner | Inner circle — the friends who remember everything |
| **Witness** | Light node pact partner | Extended circle — they know what you've been up to recently |
| **Guardian** | Bootstrap sponsor | Community patron — vouches for newcomers they don't personally know |
| **Seedling** | Bootstrapping newcomer | New arrival — growing into the community with initial support |
| **Herald** | Relay operator | Town crier — curates and distributes information beyond the local circle |

For detailed definitions, see the [Glossary](../glossary.md). For the full structural analysis, see the [protocol paper](../papers/gossip-storage-retrieval.md) §2.

## Why Not Just Nostr?

Nostr gets identity and censorship-resistance right. But:

- No native multi-device — sharing a private key across devices is the current norm
- No user sovereignty over indexing or storage — fully relay-dependent
- No efficient light node sync
- Follow-as-commitment model doesn't exist — no incentive to curate

Gozzip builds a thin layer on top of Nostr's proven primitives to solve these gaps without breaking compatibility. Existing Nostr keys, events, and relays work unchanged. Migration is incremental — a Nostr user becomes a Gozzip user by installing a client that adds device delegation and storage pacts in the background.

## Data Portability and Interoperability

Users own their data — it's stored on their devices and their social graph, not locked into any protocol. Because Gozzip events are self-authenticating (signed by the author's keys), they can be verified regardless of where they're read.

**Nostr interoperability** is native — Gozzip IS Nostr events with additional kinds. Any Nostr client reads Gozzip content. Any Nostr relay stores Gozzip events. The bridge is the identity: same keys, same event format, same relay protocol.

**Cross-protocol portability** — Gozzip events are structured data (signed JSON) that can be converted to and from:

- **ActivityPub (Mastodon, Pleroma, Misskey)** — Gozzip events map to ActivityPub activities (Create/Note, Follow, Like, Announce). A bridge translates between formats while preserving authorship via linked signatures.
- **AT Protocol (Bluesky)** — similar mapping: Gozzip events → AT Protocol records. Identity bridging via DID-to-pubkey resolution.
- **RSS/Atom** — Gozzip feeds export as standard syndication feeds. Read-only but universal.

The protocol is not Nostr-specific. It's a data ownership and storage layer that happens to speak Nostr natively because Nostr has the best primitives for censorship-resistant identity. If a better foundation emerges, the data moves — it belongs to the user, not the protocol.
