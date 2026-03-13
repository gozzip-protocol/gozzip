# Surveillance Surface Analysis

**Date:** 2026-03-12
**Status:** Draft

## Overview

This document maps what different adversaries can observe about Gozzip users depending on device type, network position, and transport model. The goal is to be honest about what the protocol hides, what it leaks, and where the outbox model provides structural protection.

## How the outbox model works

Gozzip inherits Nostr's outbox model. Users don't connect to each other — they connect to relays.

```
Alice (phone) ──→ Relay A ←── Bob (desktop)
                  Relay B ←── Carol (laptop)
```

- Alice publishes events to her preferred relays (listed in kind 10002)
- Bob knows Alice's relay list and fetches her events from one of them
- Alice and Bob never learn each other's IP
- Each relay sees only the traffic that passes through it

This is the default communication path for gossip, event publishing, and data requests. Direct peer connections (NAT hole punching, direct TCP) are the exception, not the rule.

### How the outbox model applies to NAT hole punching

Even when peers do establish direct connections (for bulk pact sync, for example), the signaling — the exchange of IP addresses needed to hole-punch — goes through relays. And crucially, each pact partner relationship can use a different relay for signaling:

```
Alice has 20 pact partners:
  Pact #1  (Bob)   → signaling via Relay A
  Pact #2  (Carol) → signaling via Relay B
  Pact #3  (Dave)  → signaling via Relay C
  Pact #7  (Eve)   → signaling via Relay A (same relay, different pair)
  Pact #14 (Frank) → signaling via Relay D
  ...
```

No single relay sees all 20 hole-punch exchanges. Each relay sees maybe 3-5 signaling pairs that happen to use it. To reconstruct Alice's full pact partner set from signaling metadata alone, every relay involved would need to collaborate — and even then, the signaling payloads are encrypted, so the relay knows "these two clients exchanged something" but not "these two are pact partners."

After hole punching, the direct connections are spread across the internet — different ISPs, different jurisdictions. No single ISP sees all 20 direct connections unless all pact partners happen to be on the same network (unlikely given the protocol's geographic diversity requirements for pact partner selection).

### What the outbox model prevents

- **IP exposure between social contacts.** Alice's followers never connect to Alice's device. They connect to Alice's relays.
- **Full graph visibility from any single point.** Each relay sees a slice — who publishes to it and who reads from it. No relay sees every user's full follow list or pact partner set. The same fragmentation applies to NAT hole-punch signaling.
- **Correlation of sender and receiver.** Even within a single relay, rotating-token data requests (kind 10057) hide whose data is being fetched. The relay sees "someone requested data matching hash X" — not "Bob asked for Alice's events." The request token is a daily-rotating lookup key computed as H(target_pubkey || YYYY-MM-DD). It prevents casual observers from linking requests across days but is trivially reversible by any party that knows the target's public key.
- **Pact topology mapping from signaling.** NAT hole-punch signaling is distributed across relays following the outbox model. Each relay sees a fraction of a user's pact connections — not the full set.

### What the outbox model does not prevent

- **Per-relay partial graphs.** A relay operator sees which pubkeys publish to them and which clients fetch from them. Over time, read patterns can suggest follow relationships.
- **Relay convergence.** If most users use the same 3-5 popular relays, those operators see a disproportionate share of traffic. The protocol supports relay diversity; whether it happens depends on deployment.
- **Timing correlation.** If Alice publishes at 14:03:01 and Bob fetches at 14:03:04 from the same relay, the operator can infer a relationship. Similarly, NIP-46 store/fetch timing can suggest pact pairings even though the payload is encrypted. Partially mitigated by batch fetching, polling intervals, and jittered NIP-46 operations. However, a persistent relay operator observing for 2-4 weeks can probabilistically identify pact pairs despite jitter.
- **Partial pact inference from signaling.** A relay that handles hole-punch signaling for 3-5 of a user's pact partners can infer those specific relationships — but not the other 15-17.

## Surveillance surface by device type

### Full node (Keeper) — desktop, home server, or VPS

**Profile:** ~95% uptime, static or semi-static IP, 20 active pact partners + 3 standby, serves data to followers.

**What a network observer (ISP) sees:**
- Persistent connections to relay servers (WebSocket)
- Volume and timing of relay traffic
- That this device is running a Gozzip client (unless using Tor/VPN)
- If NAT hole punching is used: direct connections to pact partners' IPs (but spread across different ISPs — see outbox model section)

**What a network observer does NOT see:**
- Which pubkeys this user interacts with (relay traffic is encrypted; direct connections reveal peer IPs but not identities)
- That direct connections are pact relationships (the ISP sees IP pairs, not the application-layer meaning)
- Content of any events (encrypted transport)
- Social graph structure (though direct connection patterns are partial evidence)

**What a single relay operator sees:**
- The IP address of every client that connects to this relay
- This pubkey publishes events regularly (high uptime, consistent)
- Certain clients fetch events authored by this pubkey
- Encrypted NIP-46 blobs stored/fetched by this pubkey (pact communication — but content is opaque)
- Timing correlation between NIP-46 store and fetch operations (can suggest which clients are communicating, even though payload is encrypted)

**What a relay operator does NOT see:**
- Who this user's pact partners are with certainty (NIP-46 payloads are encrypted — but timing correlation provides probabilistic evidence)
- Whose data is being requested in rotating-token data requests (token rotation daily)
- The user's full follow list (only the follows fetched through this specific relay)
- Content of DMs (NIP-44 encrypted)

**What pact partners see:**
- That they have a pact with this user (they agreed to it)
- Challenge-response interactions proving data possession
- If using relay-mediated channels: nothing beyond encrypted blobs (but the relay mediating the channel can observe timing patterns between the two parties)
- If using direct connections for bulk sync: the user's IP during sync sessions

**Risk profile:** Moderate exposure. Even when direct connections are used for bulk pact sync, the outbox model fragments the surveillance surface: NAT hole-punch signaling for each pact partner goes through a different relay, and the resulting direct connections are spread across different ISPs and jurisdictions. No single relay sees all 20 pact signaling exchanges; no single ISP sees all 20 direct connections. A pact partner who connects directly learns this user's IP — but only their own relationship, not the other 19. However, with N pact partners using direct connections, N independent entities learn the user's IP. Each can independently associate the pubkey with a physical location. In adversarial environments, relay-mediated channels should be preferred to avoid this cumulative exposure.

### Light node (Witness) — phone, laptop

**Profile:** ~30% uptime, dynamic IP (cellular/WiFi), fewer pact partners, syncs in bursts.

**What a network observer (ISP) sees:**
- Intermittent connections to relay servers
- Burst patterns (sync on app open, then idle)
- That this device runs a Gozzip client (unless using Tor/VPN)

**What a network observer does NOT see:**
- Social graph (relay-mediated)
- Pact partners (NIP-46 encrypted)
- Content

**What a single relay operator sees:**
- The IP address of every client that connects to this relay
- A client that connects periodically
- Fetches events from certain pubkeys (partial follow list visible to this relay)
- Publishes events intermittently

**What a relay operator does NOT see:**
- Full follow list (only the portion fetched through this relay)
- Pact topology with certainty (timing correlation is partial and probabilistic)
- Content of DMs
- Rotating-token request targets

**Risk profile:** Lower than full nodes. Dynamic IPs make network-level mapping harder. Shorter sessions mean less data for timing analysis. The main risk is relay-side read pattern inference — if this client always fetches the same 30 pubkeys from the same relay, the relay can infer the follow list over time. Mitigation: spread follows across multiple relays, batch fetches.

### BLE-only device (BitChat mesh)

**Profile:** No internet connection, communicates exclusively over Bluetooth Low Energy.

**What a network observer (ISP) sees:**
- Nothing. No IP traffic.

**What a BLE proximity scanner sees:**
- Bluetooth advertisements from nearby devices
- Encrypted Noise Protocol sessions between peers
- That BLE mesh traffic is occurring in this physical area

**What a BLE proximity scanner does NOT see:**
- Content (Noise Protocol encrypted)
- Identity (ephemeral subkeys for nearby mode, not linked to root pubkey)
- Social graph (opaque payloads relayed without understanding)

**What BLE mesh peers see:**
- That a device is nearby and participating in the mesh
- Encrypted packets being relayed (opaque if not the intended recipient)
- If using ephemeral keys for nearby mode: a temporary pubkey that is discarded after deactivation

**Risk profile:** Minimal digital surveillance surface. The primary risk is physical — BLE has ~100m range, so participation in a mesh reveals physical presence in a location. Ephemeral subkeys prevent linking presence to persistent identity. The risk is being identified as "someone using encrypted BLE mesh in this area," not as a specific person.

### Multi-device user (typical)

**Profile:** A phone (Witness) + desktop (Keeper), both linked to the same root identity via device delegation.

**What a network observer sees:**
- Two devices connecting to relay servers, possibly from different IPs/networks
- No visible link between the two devices (delegation is in the application layer, not the transport layer)

**What a relay operator sees:**
- Events from two different device subkeys
- Both listing the same root pubkey in `root_identity` tags
- The relay can link the two devices to one identity (this is by design — followers need to resolve device → root)

**Risk profile:** The relay-visible link between devices is intentional and necessary for multi-device sync. It reveals "these two devices belong to the same person." This is a trade-off: multi-device convenience vs. device unlinkability. Users who need device unlinkability should use separate identities.

## Surveillance surface by adversary type

### Passive network observer (ISP, backbone tap)

**Sees:** IP connections to relay servers. Traffic volume and timing. If NAT hole punching is used: direct connections to peer IPs (fragmented across ISPs by the outbox model).
**Does not see:** Content, social graph, pubkeys, rotating-token requests. Whether direct connections are pact relationships or something else.
**Can infer:** That someone uses Gozzip (relay connection fingerprinting). Which relays they use. That direct peer connections exist (but not their application-layer purpose).
**To go further, needs:** Relay operator cooperation or traffic analysis across ISPs.

### Single relay operator

**Sees:** IP addresses of connecting clients. Pubkeys that publish to them. Clients that fetch from them. Encrypted NIP-46 blobs. Rotating-token data request hashes. Timing of all operations.
**Does not see:** Content of encrypted events. Pact topology. Full social graph. Rotating-token request targets.
**Can infer:** Partial follow relationships (from read patterns over time). Approximate posting frequency per pubkey. Possible pact pairings (from NIP-46 store/fetch timing correlation).
**To go further, needs:** Cooperation with other relay operators to merge partial views.

### Colluding relay operators

**Sees:** Merged view of multiple relays. IP addresses of all their respective clients. Broader read/write patterns. Combined NIP-46 timing data.
**Can infer:** More complete follow graphs. Cross-relay activity patterns. Stronger pact pairing candidates (from merged NIP-46 timing across relays). Rotating-token request targets — the token H(target_pubkey || YYYY-MM-DD) is trivially reversible by any relay that knows the pubkey set. Colluding relays with a directory of known pubkeys can resolve every request token to its target identity.
**Still cannot see:** Encrypted content. Pact partner identities with certainty (NIP-46 encrypted — timing correlation is probabilistic, not definitive).
**To go further, needs:** Break NIP-44/NIP-46 encryption or compromise user devices.

### Physical proximity attacker (BLE scanning)

**Sees:** Bluetooth advertisements. Encrypted mesh traffic.
**Does not see:** Identity (ephemeral keys). Content (Noise Protocol). Social relationships.
**Can infer:** "Someone is using encrypted BLE mesh nearby."
**To go further, needs:** Physical device access.

### Compromised pact partner

**Sees:** Events stored under the pact. Challenge-response interactions. If direct connection was used: the user's IP during sync.
**Does not see:** Other pact partners. Events stored with other peers. The user's full social graph.
**Can do:** Withhold data (detected by checkpoint cross-verification). Share stored events with third parties (events are already signed and could be public).
**Cannot do:** Forge events (no access to signing keys). Decrypt DMs (separate key). Modify stored events (signature verification fails).

### DM metadata exposure

NIP-59 gift wraps expose both sender and recipient to the relay operator. The sender is identifiable via the signing device pubkey (resolvable to root identity through kind 10050). The recipient's pubkey appears in the `p` tag, required for relay routing. The relay therefore knows both sides of every DM exchange.

**What a relay operator handling DM traffic sees:**
- Sender identity (device pubkey → root identity via kind 10050)
- Recipient identity (`p` tag)
- Timing and frequency of DM exchanges
- Response patterns (Alice sends, Bob responds 3 minutes later)

**What a relay operator does NOT see:**
- DM content (NIP-44 encrypted)
- Content of any media references within DMs

**Mitigation options (not yet implemented):**
- DM relay rotation — use different relays for DMs to different recipients
- Batch DM delivery — queue and publish in fixed intervals to degrade timing precision
- Consider pseudonymous recipient identifiers for DM routing (requires recipients to poll rather than relay-route)

## Summary table

| Adversary | Sees IP? | Sees social graph? | Sees content? | Sees pact topology? |
|-----------|----------|-------------------|---------------|-------------------|
| ISP / network tap | Yes (to relays; to peers if hole-punching) | No | No | No (but direct connection patterns are partial evidence) |
| Single relay operator | Yes (connecting clients) | Partial (read patterns) | No (encrypted) | Probabilistic (NIP-46 timing correlation) |
| Colluding relays | Yes (their respective clients) | ~80% if 3-5 popular relays serving majority of users collude | No | Stronger probabilistic (merged timing) |
| BLE scanner | No | No | No | No |
| Compromised pact partner | Maybe (if direct sync) | No (only their pact) | Their pact's events only | No (only their own) |
| User's own devices | Yes | Yes | Yes | Yes |

## Key architectural properties

1. **No single point of full visibility.** No relay, ISP, or peer sees the complete picture. Reconstruction requires active collaboration between multiple adversary types.

2. **Rotating tokens degrade over time, but rotate daily.** The pseudonymous request token is computed as H(target_pubkey || YYYY-MM-DD). It prevents casual observers from linking requests across days but is trivially reversible by any party that knows the target's public key. An adversary who computes today's token starts fresh tomorrow — but the barrier is knowing which pubkey to target, not the computation itself. **Day-boundary linkage:** Storage peers match requests against both today's and yesterday's date to handle clock skew (see ADR 008, §9). A storage peer that logs matched tokens can link a request using yesterday's token to a request using today's token for the same target — confirming continuity across the day boundary. The protocol accepts this linkage as a trade-off for usability; the rotating token is a casual-observer deterrent, not a cryptographic blinding scheme.

3. **The outbox model is the primary defense.** By routing all traffic through relays, the protocol prevents direct IP exposure between social contacts. The relay sees partial metadata; the network sees only relay connections.

4. **Direct connections are fragmented by the outbox model.** Even when NAT hole punching is used, the signaling is distributed across different relays per pact partner, and the resulting direct connections are spread across different ISPs. No single observer sees all connections. Each pact partner learns the user's IP for their session only — not the rest of the pact topology. Relay-mediated channels (NIP-46) avoid even this partial exposure at the cost of latency.

5. **Physical presence is the hardest to hide.** BLE mesh participation reveals that someone is nearby using encrypted mesh. Ephemeral keys prevent identity linkage, but physical presence itself is observable. This is a fundamental limit of radio communication.

6. **Relay diversity is a deployment problem, not a protocol problem.** The protocol supports publishing to multiple relays and fetching from different ones. If users converge on few relays, those operators gain disproportionate visibility. This mirrors the web: HTTPS is encrypted, but if everyone uses the same CDN, that CDN sees traffic patterns.

## Permanent Identity Linkage

Every published event is permanently and irrevocably linked to the author's pubkey via cryptographic signature. This is a fundamental architectural property with significant privacy implications:

1. **Non-repudiability.** Unlike Signal's deniable messages, Gozzip events carry non-repudiable signatures. A signed event proves authorship to any third party. This applies to public posts, reactions, reposts, and DMs (the outer gift wrap is signed).

2. **Retroactive deanonymization.** If a pseudonymous pubkey is ever linked to a real-world identity — through a KYC exchange, a slip in operational security, or legal compulsion — ALL historical events signed by that pubkey are attributed. The entire posting history becomes identified retroactively.

3. **Cross-protocol portability of attribution.** Because events are self-authenticating signed JSON, they can be rebroadcast to other networks with intact signatures. A signed post can serve as evidence outside the Gozzip/Nostr ecosystem.

4. **No expiration.** Signed events have no built-in expiration. Events stored by pact partners, relays, or anyone who received them persist indefinitely with intact authorship proof.

This is an inherent property of any protocol built on cryptographic signatures and is shared with Nostr. It is the opposite of deniability-by-design systems like Signal. Users who require deniability should use purpose-built tools for sensitive communications.

## Privacy Non-Goals

Gozzip is a censorship-resistant social networking protocol with practical privacy improvements over existing social platforms. It is NOT an anonymous communication tool. To set accurate expectations:

- **Gozzip does not provide anonymity.** Every event is permanently linked to the author's pubkey.
- **Gozzip does not provide content deniability.** Signed events are non-repudiable and can be verified by any third party.
- **Gozzip does not provide read privacy against motivated adversaries.** The rotating request token is computationally trivial to reverse for any party that knows the target pubkey — including relay operators.
- **Gozzip does not provide forward secrecy for public content.** Public events are permanently signed and attributable.
- **Gozzip does not protect against relay collusion.** Three to five colluding relay operators serving a majority of users can reconstruct near-complete social graphs from metadata.
- **Gozzip does not hide DM communication graphs.** Relay operators see sender and recipient of every DM, even though content is encrypted.

**What Gozzip does provide:**
- Censorship resistance through distributed storage (no single party can delete your data)
- Key hierarchy that contains device compromise
- Encrypted DMs (content privacy, not metadata privacy)
- WoT-bounded gossip that limits information propagation to trusted peers
- Relay fragmentation that raises the cost of comprehensive surveillance
- BLE mesh for local communication without internet

**For users facing state-level adversaries, use purpose-built privacy tools:** Tor for anonymous browsing, Signal for private messaging, Briar for metadata-resistant communication. Gozzip complements these tools but does not replace them.
