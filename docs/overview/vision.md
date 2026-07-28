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
- Data storage via reciprocal WoT pacts (reduced relay custody, not eliminated relay dependency)
- Incentives are emergent — reliable storage and good curation earn wider reach, no tokens or subscriptions. See [ADR 009](../decisions/009-incentive-model.md).

The user opens the app and sees their feed. That's it.

## Community Parallel

The protocol's design mirrors human social dynamics. Each protocol role maps to an observable pattern in how communities form, maintain relationships, and propagate information.

| Persona | Protocol Role | Human Equivalent |
|---------|--------------|------------------|
| **Keeper** | Full node pact partner | The friends who remember everything (Dunbar's inner layers) |
| **Witness** | Light node pact partner | Extended circle — they know what you've been up to recently |
| **Guardian** | Bootstrap sponsor | Community patron — vouches for newcomers they don't personally know |
| **Seedling** | Bootstrapping newcomer | New arrival — growing into the community with initial support |
| **Herald** | Relay operator | Town crier — curates and distributes information beyond the local circle |

### How a person grows into the network

The protocol is the community-formation story, made executable. A single user's arc runs along three ladders that turn together:

1. **Who you are to others (personas).** You arrive as a **Seedling**. Nobody in the network can vouch for you yet, so a **Guardian** — an established member volunteering one slot of storage — sponsors you, the way a neighbor co-signs a lease for someone they barely know. As you follow people and they follow back, your friends become your **Keepers** (always-on, they remember your whole history) and **Witnesses** (they hold your recent events). Beyond your circle, **Heralds** — relay operators — carry your reach to strangers. In time you become someone else's Keeper, and eventually a Guardian yourself.

2. **How you communicate (phases).** You start **relay-dependent** — the town square is where everything happens (**Bootstrap**, 0–5 pacts). As pacts accumulate, more of your data lives on your friends' devices and less on relays (**Hybrid**, 5–15). Once your social graph is dense enough to be your own infrastructure, relays shrink to discovery and bootstrap (**Sovereign**, 15+). Relay dependency decays as the graph grows — it is never a gate, always a gradient.

3. **How others reach you (feed tiers).** Strangers don't start in your inner circle; they earn their way in. An unknown author first appears in your **Horizon** (weak ties, relay-discovered). If people you already trust interact with them, they rise into your **Orbit**. Sustained mutual interaction promotes them to your **Inner Circle** — the mutual follows whose content is pact-guaranteed and always available. Content is promoted along this ladder by observed behavior, the protocol's version of earning a place in the community.

The three ladders are the same movement seen from three angles, and one line ties them together: **Bootstrap-phase users see mostly Horizon (relays), Hybrid users watch their Orbit grow, and Sovereign users see mostly their Inner Circle.** The engine that keeps all of it turning is generosity paid forward — today's Seedling, remembered and vouched for, becomes tomorrow's Guardian. Reciprocal pacts are reciprocal friendship; reliability is reputation earned through behavior; guardianship is patronage; the whole thing is a village writing itself into a protocol.

For canonical definitions of every term, see the [Glossary](../glossary.md). For the full structural analysis — Dunbar circles, reciprocity as infrastructure, gossip as curated propagation — see the [protocol paper](../papers/gossip-storage-retrieval.md) §2.

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

**Cross-protocol portability** — Public content (posts, reactions, reposts) can be exported to ActivityPub, AT Protocol, and RSS/Atom via bridge services. Protocol-specific features (pacts, WoT routing, device delegation, encrypted DMs) are not bridgeable.

- **ActivityPub (Mastodon, Pleroma, Misskey)** — public Gozzip events map to ActivityPub activities (Create/Note, Follow, Like, Announce). A bridge translates between formats while preserving authorship via linked signatures.
- **AT Protocol (Bluesky)** — similar mapping for public events: Gozzip events → AT Protocol records. Identity bridging via DID-to-pubkey resolution.
- **RSS/Atom** — Gozzip feeds export as standard syndication feeds. Read-only but universal.

The protocol is not Nostr-specific. It's a data ownership and storage layer that happens to speak Nostr natively because Nostr has the best primitives for censorship-resistant identity. If a better foundation emerges, the data moves — it belongs to the user, not the protocol.

## Day-One Value (Before Pacts Exist)

The pact layer is Gozzip's long-term value proposition, but it requires network density to function. These features work from the moment a user installs the client, with zero pacts formed:

**Multi-device identity** — Root key + device delegation (kind 10050) is a pure improvement over Nostr's shared-key model. Users run multiple devices without copying private keys between them. Device compromise is contained — revoke the device, keep the identity. This works from the first event.

**Encrypted DM threading with key separation** — DM encryption targets a dedicated DM key (derived from its own independent seed, rotated every 90 days) rather than the root key itself. Per-device DM capability flags limit which devices can read messages. This adds DM key separation and per-device capability flags on top of NIP-44 — a meaningful improvement over sharing the identity key across devices, available from day one. (It is periodic key rotation, not forward secrecy — see [ADR 018](../decisions/018-honest-security-and-relay-framing.md).)

**Social recovery** — Users can designate recovery contacts (kind 10060) immediately after creating their identity. N-of-M threshold recovery with a 7-day timelock works as soon as the recovery contacts are published. No pact network needed.

**Per-device hash chains** — Every device-signed event carries a `seq` and `prev_hash` tag, creating a per-device hash chain. Event integrity verification (gap detection, tamper detection) works from the first event. Pacts add storage redundancy later; the integrity mechanism is independent.

**Fork-detecting profile/follow merge** — The `prev` tag on replaceable events (kind 0, kind 3) enables automatic fork detection and deterministic merge. Multi-device editing of profiles and follow lists just works — no coordination server, no conflict resolution UI. This is a standalone improvement over Nostr's last-write-wins model.

**The pact layer is a background benefit that accrues over time.** Users do not wait for pacts to form before gaining value. As the user builds their follow graph, bootstrap pacts form automatically. As the WoT matures, reciprocal pacts replace bootstrap pacts. Storage sovereignty emerges gradually — the user never has to think about it.

**Position: the best Nostr client, with sovereignty as a gradient.** The first Gozzip client should be adopted because it is the best Nostr client available — multi-device, better DMs, social recovery, hash chain integrity. Data sovereignty is not a gate that users must pass through; it is a gradient that increases as the network grows. Users who install the client on day one get a better Nostr experience immediately. Sovereignty accrues in the background.

## Reference Library

A reference library (`gozzip-core`) is a mandatory deliverable. Gozzip's client-side complexity — pact negotiation, challenge-response, WoT graph computation, device resolution, tiered retrieval — spans 20+ interacting systems, where a basic Nostr client implements a handful. Without a shared library, every client must re-implement those systems from prose specifications.

The reference library encapsulates the protocol stack as a Rust crate with TypeScript/WASM bindings. Client developers import it and build UIs on top. This is how the Nostr ecosystem works in practice — `nostr-tools` (TypeScript) and `rust-nostr` (Rust) power the majority of clients. Gozzip follows the same pattern at a higher complexity level.
