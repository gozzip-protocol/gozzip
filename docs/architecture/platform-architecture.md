# Platform Architecture

How Gozzip runs on desktop, web, and mobile. Each platform uses a shared Rust protocol core (`gozzip-core`) compiled to the appropriate target. The protocol works with standard Nostr relays and clients from day one — no custom infrastructure required.

> **Phase 1 focus: desktop and web (browser extension).** Mobile support is designed into the protocol but not the current implementation target. Everything below about mobile describes the protocol's capability — it will be implemented in Phase 2.

## Executive Summary

Gozzip's light node profile (60% uptime, 4.14 MB/day, 30-day storage window) fits desktop and browser extension users well. Desktop is straightforward. Web via browser extension is the most compelling near-term strategy — extensions have background service workers, persistent storage, and can maintain WebSocket connections, making them capable light nodes without a separate daemon.

**Platform priorities (Phase 1 — desktop/web/extension):**

1. Browser extension (light node + NIP-07 signer) — fastest path to users
2. Desktop app (full node via Tauri 2.0) — proves the protocol end-to-end

**Phase 2 — mobile:**

3. Pact Proxy daemon (enables mobile) — same Rust codebase
4. Android app (fewer background restrictions)
5. iOS app (most constrained, needs proxy to be solid)

---

## 1. Desktop: Excellent Feasibility

No background restrictions, generous storage/bandwidth, persistent network access. The existing Rust codebase compiles natively.

| Aspect | Assessment |
|--------|-----------|
| Background connectivity | Unrestricted — maintain pact partner connections 24/7 |
| Storage | Full node viable: ~50 MB/month for 20 pacts at power-user rates |
| Bandwidth | 11.68 MB/day (full node) is negligible |
| Crypto | secp256k1 + SHA-256 at native speed |
| Precedent | **Gossip** (Rust + LMDB + egui) proves Rust desktop Nostr clients work |

**Recommended stack:** Tauri 2.0 — Rust backend (embed Gozzip protocol core directly) + web frontend. Single codebase produces macOS, Windows, Linux binaries. Protocol logic stays in Rust with zero FFI overhead.

**Alternative:** Pure Rust with egui (like Gossip client) for a power-user tool.

---

## 2. Web via Browser Extension: The Strategic Play

### Why an Extension Changes Everything

A regular web app can only run when its tab is open — no pact obligations, no gossip forwarding, no challenge responses. A browser extension flips this:

| Capability | Web App (tab) | Browser Extension |
|-----------|--------------|-------------------|
| Background execution | None | Service worker + offscreen document |
| Persistent storage | ~50-100 MB IndexedDB | IndexedDB (gigabytes) + chrome.storage |
| WebSocket connections | Tab must be open | Maintained by service worker (Chrome 116+) |
| Pact obligations | Impossible | Feasible (answer challenges, serve events) |
| Key management | Must ask user each time | Stored securely, NIP-07 API |
| Cross-site availability | One origin only | Injected into any Nostr web app |

**The extension acts as a local light node**, similar to how MetaMask is an Ethereum node in your browser. Any Nostr web app (Snort, Coracle, Iris, or a Gozzip web UI) talks to it via the standard NIP-07 `window.nostr` interface, extended with Gozzip-specific methods.

### Architecture

```
┌──────────────────────────────────────────────────────┐
│  Browser Extension                                    │
│                                                       │
│  ┌────────────────────┐  ┌─────────────────────────┐ │
│  │  Service Worker     │  │  Offscreen Document     │ │
│  │                     │  │  (keepalive fallback)   │ │
│  │  • WASM protocol    │  │                         │ │
│  │    core (Rust)      │  │  • Pings worker every   │ │
│  │  • Gossip routing   │  │    25s if WS is idle    │ │
│  │  • Pact management  │  │                         │ │
│  │  • WoT scoring      │  │                         │ │
│  │  • Feed tier logic  │  │                         │ │
│  │  • Challenge resp.  │  │                         │ │
│  └────────┬───────────┘  └─────────────────────────┘ │
│           │                                           │
│  ┌────────▼───────────┐  ┌─────────────────────────┐ │
│  │  IndexedDB          │  │  Content Script          │ │
│  │                     │  │                          │ │
│  │  • Stored events    │  │  Injects window.nostr   │ │
│  │  • Pact state       │  │  + window.gozzip into   │ │
│  │  • WoT graph        │  │  web pages              │ │
│  │  • Interaction data │  │                          │ │
│  │  • Checkpoint state │  │  Bridges page ↔ worker   │ │
│  └─────────────────────┘  └─────────────────────────┘ │
│                                                       │
│  WebSocket connections to pact partners / relays      │
└──────────────────────────────────────────────────────┘
         ▲                              ▲
         │ NIP-07 + Gozzip API          │ WS heartbeat 20s
         ▼                              ▼
┌──────────────────┐          ┌──────────────────┐
│  Any Nostr Web   │          │  Pact Partners   │
│  App (Snort,     │          │  & Relays        │
│  Coracle, etc.)  │          │                  │
└──────────────────┘          └──────────────────┘
```

### Service Worker Lifecycle

MV3 service workers terminate after **30 seconds of inactivity**. Keepalive strategies (ranked by reliability):

| Strategy | Mechanism | Notes |
|----------|-----------|-------|
| WebSocket heartbeat | Send/receive every 20s | Chrome 116+: WS traffic resets idle timer natively |
| Offscreen document | Pings worker every 25s | Independent lifetime, Chrome 109+ |
| chrome.alarms | 30s interval wake-up | Weakest — worker sleeps between alarms |
| Native messaging | connectNative() | Strongest — cancels both timeouts. Requires native binary |

For Gozzip: **WebSocket heartbeat** (pact partner connections naturally provide this) + **offscreen document** as fallback. The gossip protocol's regular message flow keeps the worker alive organically.

**Design for ephemerality:** The worker will restart. WASM modules need re-instantiation. WebSocket connections will drop. All critical state must live in IndexedDB. The protocol core should treat the worker as a process that can crash at any time and reconstruct working state from storage within milliseconds.

### WASM Integration

The Rust protocol core compiles to WASM and runs directly in the extension:

- Compile with `wasm-bindgen` (not wasm-pack — archived July 2025)
- Bundle `.wasm` file in extension package (MV3 prohibits remote code)
- Requires CSP: `"script-src 'self' 'wasm-unsafe-eval'"`
- Re-instantiate on each worker restart (sub-100ms for <1 MB binary)
- Crypto: secp256k1 via `k256` crate or `libsecp256k1` WASM binding (WebCrypto doesn't support secp256k1)

### NIP-07 Compatibility + Gozzip Extensions

```typescript
// Standard NIP-07 (works with all Nostr web apps)
window.nostr = {
  getPublicKey(): Promise<string>,
  signEvent(event): Promise<SignedEvent>,
  nip04: { encrypt, decrypt },
  nip44: { encrypt, decrypt },
};

// Gozzip extensions (for Gozzip-aware web apps)
window.gozzip = {
  getPactStatus(): Promise<PactSummary[]>,
  getWotTier(pubkey: string): Promise<"inner-circle" | "orbit" | "horizon" | "unknown">,
  getFeed(options): Promise<Event[]>,
  getInteractionScore(target: string): Promise<number>,
  getStorageStats(): Promise<{ used: number, capacity: number }>,
};
```

### Cross-Browser Support

| Feature | Chrome | Firefox | Safari |
|---------|--------|---------|--------|
| Background context | Service worker | Event page (more forgiving) | Both supported |
| WebSocket keepalive | Chrome 116+ native | Background page holds WS directly | Background page |
| Offscreen documents | Yes (Chrome 109+) | Not supported (not needed) | Not supported |
| WASM in background | Yes (with CSP) | Yes | Yes |
| MV2 support | Removed mid-2025 | Indefinitely supported | Available |

Firefox is actually easier — its event-page model gives a DOM context and more forgiving lifecycle. A cross-browser extension uses `background.scripts` for Firefox/Safari and `background.service_worker` for Chrome.

### Storage Budget

| Data | Size | Storage |
|------|------|---------|
| Pact state (20 active + 3 standby) | <100 KB | IndexedDB |
| Stored events (light node, 30-day window) | 15-50 MB | IndexedDB |
| Read cache (LRU) | 100 MB configurable | IndexedDB |
| WoT graph + interaction data | 1-5 MB | IndexedDB |
| Signing keys | <1 KB | chrome.storage.local (encrypted) |

IndexedDB quota: Chrome allows up to 60% of available disk space per origin. More than enough.

### What the Extension Replaces

With the extension running as a light node, the web story becomes:

- **No separate daemon needed** for web users (unlike the Pact Proxy required for mobile)
- **Any Nostr web app** becomes Gozzip-aware via the injected API
- **Pact obligations are met** even when no Gozzip tab is open
- **Key management is solved** by NIP-07 (users already understand this from nos2x/Alby)

---

## 3. Mobile: Phase 2

> **Not targeted in Phase 1.** The protocol is designed to support mobile-to-mobile pacts, but the current implementation focuses on desktop and browser extension. This section documents the protocol's mobile capability for future implementation.

### The Reality

90%+ of social network users are on mobile. This means mobile-to-mobile pacts will eventually be the **majority of all pacts**. The architecture treats mobile as a first-class platform — but implementation is Phase 2, after desktop/web/extension proves the protocol end-to-end.

### OS Constraints

| Constraint | iOS | Android |
|-----------|-----|---------|
| Persistent background connections | **Impossible** | ForegroundService (with notification) |
| Background processing | BGAppRefreshTask: ~30s, unpredictable | WorkManager: restricted by Doze |
| BLE mesh (iroh Tier 0) | CoreBluetooth (limited background) | Full access |
| Push notifications | APNS required | FCM recommended |

**Critical fact:** Briar (pure P2P messenger) has no iOS version and never will — iOS kills background Bluetooth/network. Gozzip must not repeat this mistake.

### Resource Fit

Gozzip's light node profile fits mobile well:

| Resource | Gozzip Light Node | Mobile Budget |
|----------|-------------------|---------------|
| Bandwidth | 4.14 MB/day (~124 MB/month) | Well within data plans |
| Storage | ~15 MB/month (20 pacts) + 100 MB cache | Comfortable |
| Uptime | 30% expected (see note below) | Matches typical app usage |
| CPU | Event signing, hash challenges | Negligible |

**Uptime note:** The 30% figure assumes active app usage. Mobile OS constraints (iOS BGAppRefreshTask, Android Doze) limit true background uptime to 0.3-5% depending on OS version and user behavior.

### How Mobile-to-Mobile Pacts Actually Work

Two phones can't connect directly to each other — dynamic IPs, NAT, OS killing background connections. So pact messages route through **relays** that both phones connect outbound to. **Standard Nostr relays work for this without any modifications.** The relay stores events; the phone queries on reconnect. That's it.

#### How It Works With Standard Relays

**Standard Nostr relays already persist events.** When Phone A publishes an event, the relay stores it. When Phone B reconnects (hours later), it queries `REQ` with `since: <last_seen_timestamp>` and gets everything it missed. This IS the "mailbox" — no special buffering needed.

**Mode 1: Real-time forwarding (both phones online)**

When both phones are connected to the same relay simultaneously, the relay forwards messages instantly via subscriptions. Zero extra storage.

```
Phone A ──event──► Relay ──subscription──► Phone B
Phone B ──challenge──► Relay ──subscription──► Phone A
```

**Mode 2: Store-and-query (one phone offline)**

When Phone B is offline, the relay stores events normally. Phone B gets them on reconnect.

```
08:00  Phone A publishes 3 events
       Relay stores them (standard behavior)
       Phone B is asleep

14:00  Phone B wakes up, connects to relay
       Phone B: REQ since: <last_timestamp>
       Relay delivers stored events
       Phone B stores them locally (pact obligation fulfilled)
```

Clients verify relay reliability: publish event → query back to confirm storage → if the relay dropped it, publish to the next relay from NIP-65 list. Unreliable relays get removed from the user's relay list.

**What flows through relays per pact per day:**

| Message Type | Size | Notes |
|-------------|------|-------|
| Events from partner (Kind 1, 7, 6, etc.) | ~20 KB/day | Standard Nostr events |
| Challenge messages (Kind 10054) | ~300 bytes | Standard Nostr events |
| Challenge responses | ~300 bytes | Standard Nostr events |
| Pact management (Kind 10053-10058) | ~500 bytes | Standard Nostr events |
| **Total per pact partner per day** | **~20-40 KB** | **All standard relay storage** |

For a user with 20 pacts: ~400-800 KB across relays. Trivial for any relay.

**Mode 3: Checkpoint reconciliation (relay retention expired)**

If a relay's retention policy purges events before the phone reconnects, pact partners reconcile on next simultaneous online window using checkpoint comparison (Kind 10051). The checkpoint contains a merkle root of all events — they compare, identify gaps, and exchange what's missing.

```
Relay purges events after 48h, phone offline for 3 days

Day 4, both online simultaneously:
  Phone A: "My checkpoint merkle root is X, covering events 1-47"
  Phone B: "I have events 1-42 for you. Missing 43-47."
  Phone A: "Here are events 43-47."
  Phone B stores them. Gap filled. Pact healthy.
```

This is the safety net. It's slower but guarantees eventual consistency regardless of relay behavior.

#### How This Differs from Nostr Relay Storage

| | Nostr Today | Gozzip Mobile Pacts |
|---|---|---|
| **What relay stores** | All your events, permanently | Same events, same relay — but 20 peers also hold copies |
| **Custody** | Relay IS the storage layer | Relay is one of many copies; peers are the canonical store |
| **If relay drops data** | Your events may be lost | 19 other pact partners have the data; checkpoint reconciliation fills gaps |
| **If relay censors you** | You lose reach on that relay | Events flow through other relays and pact partners; no single relay is critical |
| **Trust requirement** | High — relay controls your data | Low — relay is one delivery path among many |
| **Relay code changes** | N/A | **None required** — standard Nostr relay works as-is |

**The protocol doesn't eliminate relays. It eliminates relay custody.** The relay becomes a post office, not a warehouse. Your data lives on 20 peers in your social graph, not on a server you don't control. And the relays doing this are the same unmodified Nostr relays running today.

#### The Math: Overlapping Online Windows

Two mobile nodes with 30% uptime each (assumes active app usage — the 30% figure assumes active app usage; mobile OS constraints limit true background uptime to 0.3-5%):
- Probability both online simultaneously: ~9% of any given time slot
- With 60-second ticks over 24 hours: **~130 minutes/day of overlap**
- Challenge frequency: 1/day (configurable)
- Challenge response window: generous (hours, not milliseconds)
- Event sync: 25 events/day × 800 bytes = 20 KB/day per partner

**130 minutes of daily overlap is more than enough** for challenge-response and event sync. The protocol doesn't need real-time presence — it needs periodic rendezvous.

But it's even better than 130 minutes suggests. A user with 20 pact partners has 20 independent overlap windows. Even if you miss one partner today, you'll catch the others. And each partner also has the data from their own pacts — so events propagate through the social graph even when direct overlap is unlucky.

#### Relaxed Challenge Model for Mobile

The current challenge model (respond in 500ms to prove local storage) was designed for always-on nodes. Mobile-to-mobile pacts need a different model:

| Parameter | Full Node Pact | Mobile Pact |
|-----------|---------------|-------------|
| Challenge frequency | 1/day | 1/day |
| Response window | 500ms (latency test) | 24 hours (async) |
| Challenge type | `serve` (latency) + `hash` | `hash` only (no latency test) |
| Event delivery | Real-time push | Batch sync on overlap or mailbox |
| Verification | Continuous presence | Checkpoint reconciliation |

**Hash challenges work asynchronously:** Challenger sends nonce via relay mailbox. Partner responds with `H(events[range] || nonce)` whenever they come online next. The response proves storage regardless of when it arrives.

**Latency-based challenges (`serve` type) don't apply to mobile-to-mobile** — you can't distinguish "fetching remotely" from "phone was in pocket" when the response window is hours. Hash challenges are sufficient to prove possession.

#### A Day in the Life: Complete Mobile Pact Cycle

```
═══ Day 1 ═════════════════════════════════════════════════════

07:30 — Alice opens app (Phone A comes online)
  ├── Connects to relay-alpha and relay-beta
  ├── Relay-alpha delivers: 5 buffered events from Bob (overnight)
  ├── Relay-beta delivers: 2 buffered events from Carol
  ├── Alice stores all 7 events locally (pact obligations)
  ├── Alice publishes 1 new note ("good morning")
  │   ├── Relay-alpha buffers for Bob (offline)
  │   ├── Relay-beta buffers for Carol (offline)
  │   └── Relay-alpha buffers for 18 other pact partners
  ├── Alice finds pending challenge from Dave (hash challenge, nonce=0x3f)
  │   └── Computes H(dave_events[12..18] || 0x3f), sends response via relay
  ├── Alice issues challenge to Eve (nonce=0x7a)
  │   └── Relay buffers challenge for Eve
  └── Alice backgrounds app after 8 minutes

09:15 — Bob opens app (Phone B comes online)
  ├── Connects to relay-alpha
  ├── Receives Alice's "good morning" from mailbox
  ├── Stores it locally
  ├── Bob publishes 3 events
  │   └── Relay buffers for Alice and Bob's other pact partners
  └── Bob backgrounds after 12 minutes

11:00 — Alice opens app again
  ├── Receives Bob's 3 events from relay-alpha mailbox
  ├── Alice and Carol both online simultaneously (real-time mode)
  │   ├── Carol sends 4 events → relay forwards instantly → Alice stores
  │   ├── Alice sends 2 events → relay forwards instantly → Carol stores
  │   └── No buffering needed — both connected
  ├── Alice publishes 2 more events
  └── Alice backgrounds after 15 minutes

14:30 — Eve opens app
  ├── Receives Alice's challenge (nonce=0x7a) from relay mailbox
  ├── Computes H(alice_events[5..11] || 0x7a)
  ├── Sends response via relay → relay buffers for Alice
  └── Eve backgrounds

18:00 — Alice opens app
  ├── Receives Eve's challenge response → valid ✓ (Eve still has Alice's data)
  ├── Syncs remaining events from other pact partners
  └── Day complete

═══ End of Day 1 ═══════════════════════════════════════════════

Result:
  - Alice exchanged events with all 20 pact partners (some real-time, some mailbox)
  - 1 challenge issued, 1 challenge answered, both via relay mailbox
  - Relay's maximum buffer for Alice at any point: ~40 KB
  - Relay's average hold time per message: ~3 hours
  - No phone needed a stable IP
  - No proxy was involved
  - No relay stored anything permanently
```

#### Push Notifications Without a Proxy

Relays can handle push notifications directly:

1. Mobile node registers push token with relay (NIP-98 authenticated, encrypted)
2. Relay receives a DM or mention for the user
3. Relay sends APNS/FCM wake-up (content-free — just "new messages available")
4. Mobile app wakes, connects to relay, syncs from pact partners via relay mailbox
5. Returns to background

This is exactly what **Notepush** (Damus) does today. No proxy needed — any Herald relay can offer push notification forwarding as a service.

#### Relay Dependency: Honest Assessment

**The protocol eliminates relay custody but depends on relays for delivery.** In a 90%+ mobile network:

- Relays are the rendezvous layer for mobile-to-mobile communication
- Standard Nostr relays handle this — no modifications needed
- If all your relays go down simultaneously, you can't sync pacts until they recover (or until both phones happen to be online at the same time on a surviving relay)

**Mitigations:**
- Users connect to 3-5 relays (NIP-65). If one goes down, others continue.
- Clients verify relay storage: publish → query back → if dropped, switch relay.
- Events are sent toward all 20 pact partners across multiple relays. Even if some relays purge events, most partners get the data through other paths.
- Checkpoint reconciliation (Kind 10051) is the ultimate safety net — it catches any gaps on next mutual online window.
- Running your own relay (Herald) is cheap ($5/month VPS) and eliminates dependency on third parties.

**This is a genuine trade-off.** The protocol moves from "relays own your data" (Nostr) to "relays deliver your mail" (Gozzip). The second is strictly better — a post office losing a letter is recoverable; a warehouse burning down is not — but it's still a dependency. Crucially, the relays involved are standard Nostr relays. No custom software, no vendor lock-in.

#### When a Proxy IS Useful (Optional, Not Required)

A Pact Proxy becomes an **optimization** for power users who want:

- **Zero-miss challenges:** Proxy answers instantly instead of waiting for phone to wake
- **Faster sync:** Proxy pre-aggregates events, phone gets them in one batch
- **Higher reliability scores:** Proxy maintains 95%+ uptime vs phone's 30%
- **More pact capacity:** Proxy can handle 40+ pacts (popular user tier) while phone handles 20

But the protocol **works without one.** A mobile-only user with no proxy, no desktop, no VPS can maintain 20 pacts with other mobile users through relay-mediated message passing alone.

### Beyond Relays: Future Directions

The relay-as-mailbox model works today with existing infrastructure. But it still depends on centralized servers with stable addresses. Several paths could reduce or eliminate this dependency over time — though none are proven at scale and all carry significant uncertainty.

#### iroh Mesh: Identity-Routed Transport

iroh (peer-to-peer QUIC transport) decouples identity from transport address entirely. A user's root pubkey becomes their network address. The mesh routes messages by pubkey, not IP — meaning dynamic IPs, NAT, and address changes become irrelevant at the protocol level.

**What it would enable:**
- Direct phone-to-phone message routing without relays
- Gossip and pact messages traverse the mesh automatically
- Sessions survive transport changes (WiFi → cellular → BLE)
- Store-and-forward queuing across mesh peers

**What's uncertain:**
- iroh mesh over internet transport requires bootstrapping — you still need to find initial peers somehow. This likely means relay-like bootstrap nodes.
- Mesh routing over the open internet (UDP hole punching, STUN coordination) is battle-tested in some contexts (WebRTC, BitTorrent) but unproven for social protocol traffic at scale.
- The spanning tree routing model works well in simulations but hasn't been validated with millions of intermittent mobile nodes.
- Power consumption of mesh participation on mobile is unknown — maintaining mesh routing state may conflict with OS background restrictions.

#### BLE Mesh: Offline-First Local Networks (iroh Tier 0)

Bluetooth Low Energy mesh enables phone-to-phone communication without any internet connection. The Bitchat integration design specifies:
- BLE range: ~100m, up to 7 mesh hops (practical maximum is 3-4 hops; the primary use case is 1-hop direct exchange)
- Store-and-forward: queue up to 1,000 events or 50 MB
- Geohash-tagged ephemeral keys for proximity discovery
- Auto-drain queue when any transport becomes available

**What it would enable:**
- Pact sync between phones in physical proximity (same building, transit, cafe)
- Completely relay-independent for co-located users
- Censorship-resistant local communication (no internet needed)
- Gradual event propagation through physical movement (sneakernet at protocol level)

**What's uncertain:**
- iOS restricts CoreBluetooth in background — BLE mesh may only work while app is foregrounded
- Android allows background BLE but battery drain at multi-hop mesh depth is untested (battery impact with BLE: 10-20%)
- BLE bandwidth (~1 Mbit/s theoretical, ~100-200 Kbit/s practical) limits throughput
- Mesh routing at city scale (thousands of BLE nodes) is largely unproven
- Privacy implications of broadcasting BLE beacons continuously

#### WebRTC Direct Connections

When both phones are online simultaneously, WebRTC could establish a direct peer-to-peer connection using STUN/TURN for NAT traversal, signaled through the relay. This would bypass the relay for real-time exchange.

**What it would enable:**
- Direct phone-to-phone data transfer (higher throughput than relay-forwarded)
- Reduced relay load during simultaneous-online windows
- Encrypted direct channel not visible to relay

**What's uncertain:**
- WebRTC NAT traversal success rate varies (typically 80-90% for STUN, 100% with TURN but TURN is a relay itself)
- Mobile WebRTC connections are fragile — they break on network transitions (WiFi → cellular)
- Setup overhead (ICE negotiation) may not be worth it for the short overlap windows mobile users have
- Adds implementation complexity for marginal benefit when relay forwarding already works

#### Local-First / CRDTs

An alternative approach to eventual consistency: instead of challenge-response proofs and checkpoint reconciliation, use Conflict-Free Replicated Data Types (CRDTs) for pact state. Each phone maintains a CRDT of its event log; syncing is just merging CRDTs whenever two phones communicate, through any channel.

**What it would enable:**
- Simpler sync protocol — merge replaces reconciliation
- Transport-agnostic by nature (merge works over relay, BLE, sneakernet, anything)
- No ordering requirements — events commute

**What's uncertain:**
- CRDT metadata overhead for event logs at scale (20 pacts × months of events)
- Whether CRDTs can express the challenge-response verification model
- Performance on mobile devices with large state

#### The Honest Summary

| Approach | Eliminates Relay? | Maturity | Timeline |
|----------|------------------|----------|----------|
| Relay as mailbox (current) | No — shifts from custody to delivery | Ready now | Phase 1 |
| iroh mesh (internet transport) | Partially — still needs bootstrap | Designed, unproven | 2-3 years |
| BLE mesh (iroh Tier 0) | Yes, for local communication | Designed, unproven | 1-2 years |
| WebRTC direct | Partially — only during overlap | Mature tech, untested in this context | 6-12 months |
| Local-first / CRDTs | Transport-agnostic | Research stage | Unknown |

**The practical path:** Ship with relay-as-mailbox. It works today, uses existing Nostr infrastructure, and is strictly better than relay-as-storage. Layer in BLE mesh for local/offline scenarios. Explore iroh mesh for internet-scale relay independence as the protocol matures. Don't promise relay elimination until it's proven.

### Device-Aware Protocol: Automatic Full Node Detection

The protocol doesn't treat a user as one node with one uptime profile. A user might have a phone (30% uptime), a browser extension (50% uptime while laptop is open), and a desktop app (90% uptime). The protocol **discovers this automatically** and routes pact obligations to the most capable device — without the user configuring anything.

See [Messages](../protocol/messages.md) for the Kind 10050 `uptime` tag specification.

#### How It Works

**Step 1: Each device reports its own uptime**

Every device tracks its own online/offline history and publishes a rolling uptime summary via Kind 10050 (Device Delegation):

```json
{
  "kind": 10050,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["device", "<phone_pubkey>", "mobile", "dm"],
    ["device", "<desktop_pubkey>", "desktop", "dm"],
    ["device", "<extension_pubkey>", "extension"],
    ["uptime", "<phone_pubkey>", "0.28", "1709683200"],
    ["uptime", "<desktop_pubkey>", "0.91", "1709683200"],
    ["uptime", "<extension_pubkey>", "0.52", "1709683200"]
  ]
}
```

- `uptime` tag: device pubkey, 7-day rolling average uptime (0.0-1.0), timestamp of last measurement
- Computed locally by each device from its own online/offline log
- Published by whichever device updates Kind 10050 next (any checkpoint delegate)
- Updated at most once per day to avoid churn

**Step 2: Protocol classifies each device automatically**

Based on observed uptime, each device is classified:

| Uptime (7-day avg) | Classification | Pact Role |
|---------------------|---------------|-----------|
| 90%+ | **Full node (Keeper)** | Primary pact endpoint; answers challenges instantly; serves data requests |
| 50-89% | **Active light node** | Secondary endpoint; answers challenges when online; batch sync |
| 10-49% | **Intermittent light node** | Relay-mediated mailbox sync; async challenges |
| <10% | **Passive device** | Syncs own events only; doesn't participate in pacts |

**This classification is automatic.** When a user installs the desktop app and leaves it running, the protocol notices the uptime climbing past 90% over 7 days and automatically promotes it to full node. No configuration needed.

**Step 3: Pact partners learn the device topology**

When a pact is formed or refreshed, the pact partner receives the user's device topology via the Kind 10050 uptime tags. The pact partner's client uses this to decide how to deliver events:

```
Alice's devices:
  phone:     28% uptime → intermittent light node
  desktop:   91% uptime → full node (Keeper)
  extension: 52% uptime → active light node

Bob (Alice's pact partner) decides:
  1. Primary: send events directly to Alice's desktop (WebSocket, stable IP)
  2. Fallback: relay mailbox for Alice's phone
  3. Challenges: send to desktop first (instant response), fall back to phone (async)
```

**Step 4: Internal device mesh**

The user's own devices form an internal sync mesh. The most reliable device acts as the hub:

```
               ┌─────────────────────┐
               │  Desktop (91%)       │
               │  Primary pact node   │
               │  Full event history  │
               └──┬──────────────┬───┘
                  │              │
         sync on wake     sync on wake
                  │              │
          ┌───────▼───┐   ┌─────▼──────────┐
          │ Phone     │   │ Extension      │
          │ (28%)     │   │ (52%)          │
          │ 30-day    │   │ 30-day window  │
          │ window    │   │                │
          └───────────┘   └────────────────┘
```

- Desktop receives all pact events (always on, primary endpoint)
- Phone syncs from desktop when it wakes up (via local network or relay)
- Extension syncs from desktop when browser is open
- If desktop goes offline, phone and extension fall back to relay mailbox mode
- If desktop comes back, it catches up from relay/peers and resumes as primary

**The user experiences one identity, one feed, one set of pacts** — the protocol handles device routing transparently.

#### Automatic Demotion and Promotion

The protocol adapts as device availability changes:

**Scenario: User stops opening their laptop**

```
Week 1: Desktop uptime 91% → full node, primary pact endpoint
Week 2: Desktop uptime 65% → active light node (user traveling)
Week 3: Desktop uptime 12% → intermittent (laptop in bag)
Week 4: Desktop uptime 3%  → passive device

Protocol response:
  - Week 2: Pact partners start sending to relay mailbox as fallback
  - Week 3: Phone becomes primary pact endpoint (28% > 12%)
  - Week 4: All pact sync routes through relay mailbox for phone
  - No pacts broken, no user action needed
```

**Scenario: User sets up a VPS proxy**

```
Day 1:  New device appears in Kind 10050 with type "server"
Day 3:  Server uptime at 99% → classified as full node
Day 7:  Protocol promotes server to primary pact endpoint
        Phone and extension sync through server
        Pact partners route to server directly
```

**Scenario: User's only device is a phone**

```
Phone uptime: 28%
No other devices

Protocol behavior:
  - Phone is primary (and only) pact endpoint
  - All pacts use relay-mediated mailbox sync
  - Async hash challenges with 24h response window
  - Works fine — just lower reliability score than multi-device user
```

#### What This Means for Pact Partners

Pact partners prefer users who have at least one high-uptime device, because:
- Challenges get answered faster → higher reliability score
- Events get stored reliably → data is available when needed
- Gossip forwarding is more responsive → better content reach

But this is a **preference, not a requirement.** Mobile-only users with 28% uptime can still maintain 20 pacts. Their reliability scores will be lower (more async challenges, longer response times), but the relay-mediated mailbox model ensures pacts function.

**The incentive is organic:** Want better pact partners? Leave your desktop running, or install the browser extension. The protocol notices and rewards you with higher reliability scores automatically.

#### Privacy Implications

Publishing device uptime reveals information about the user's behavior patterns. Mitigations:

- Uptime is published as a 7-day rolling average (0.28), not a detailed online/offline log
- Only pact partners receive the Kind 10050 with uptime tags (it's not broadcast)
- Device types are generic ("mobile", "desktop", "extension", "server") — no device model or OS info
- Users can opt out of uptime reporting — protocol falls back to treating all devices as intermittent

### Mobile Stack Options

**Option A (recommended): Rust core + native UI via UniFFI**

- Gozzip protocol core in Rust
- UniFFI generates Swift and Kotlin bindings automatically
- Native UI per platform (SwiftUI on iOS, Jetpack Compose on Android)
- Precedent: rust-nostr already ships Swift + Kotlin bindings; Mozilla uses UniFFI for Firefox mobile

**Option B: React Native + uniffi-bindgen-react-native**

- Single JS/TS codebase for UI
- Rust core as Turbo Module via Mozilla's uniffi-bindgen-react-native
- NDK has first-class React Native support
- Faster to ship, slightly worse native feel

**Option C: Flutter + flutter_rust_bridge**

- Dart NDK implements inbox/outbox gossip model
- flutter_rust_bridge for Gozzip-specific Rust core
- Good performance via Impeller rendering engine

---

## 4. Pact Proxy Daemon: Phase 2

> **Not targeted in Phase 1.** The proxy becomes relevant when mobile support is implemented. Documented here for completeness.

### What It Is

A headless Rust daemon (`gozzip-proxy`) that acts as an always-on delegate for mobile nodes. It runs on the user's desktop machine, a VPS, or a shared community server. Same `gozzip-core` codebase as the extension and desktop app — just without a UI.

**The proxy is not required for mobile participation.** Mobile-to-mobile pacts work via relay-mediated message passing (see Section 3). The proxy is an optimization for power users who want higher reliability scores, faster sync, and more pact capacity.

Think of it like running your own email server vs using a shared one. Both work — one gives you more control.

### What It Does

```
gozzip-proxy (headless daemon)
│
├── Pact Obligations
│   ├── Stores events received from mobile node's pact partners
│   ├── Answers Kind 10054 challenges on behalf of mobile node
│   ├── Serves Kind 10058 data offers when peers request data
│   └── Maintains reliability score (keeps pacts healthy)
│
├── Gossip Participation
│   ├── Forwards WoT-filtered gossip (Kind 10057 data requests)
│   ├── Participates in pact formation (Kind 10055/10056)
│   └── Maintains WebSocket connections to pact partners
│
├── Event Buffering
│   ├── Receives and stores events from followed authors
│   ├── Applies feed model tier classification (IC/Orbit/Horizon)
│   ├── Queues events for mobile sync on next wake-up
│   └── Prunes buffer using light node rules (30-day window)
│
├── Push Notification Gateway
│   ├── Detects high-priority events (DMs, replies, mentions)
│   ├── Sends APNS (iOS) or FCM (Android) wake-up notification
│   ├── Mobile app wakes, connects via WebSocket, syncs delta
│   └── Supports notification preferences (DM-only, all mentions, etc.)
│
└── Sync Protocol
    ├── Mobile connects → proxy sends checkpoint delta
    ├── Mobile publishes events → proxy forwards to pact partners
    ├── Mobile updates follow list → proxy adjusts gossip subscriptions
    └── Bidirectional event reconciliation via Kind 10051 checkpoints
```

### Identity Model

The proxy does **not** hold the user's root key. Instead:

1. User's mobile app generates a **device subkey** for the proxy
2. Published via Kind 10050 (Device Delegation): `["device", "<proxy_pubkey>", "server"]`
3. Proxy signs pact-related events with its device subkey
4. Pact partners verify via the delegation chain: proxy_pubkey → root_pubkey

**New event kind needed — Kind 10063 (Proxy Delegation):**

```json
{
  "kind": 10063,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["proxy", "<proxy_device_pubkey>"],
    ["capabilities", "challenge-response", "gossip-forward", "data-serve"],
    ["push_endpoint", "<APNS/FCM token, encrypted to proxy>"],
    ["expires", "<unix_timestamp>"]
  ],
  "sig": "<signed by root key>"
}
```

This event authorizes the proxy to act on behalf of the mobile node for specific capabilities. It's time-limited and revocable (publish a new 10063 without the proxy, or let it expire).

### Deployment Options

| Option | Who It's For | Setup |
|--------|-------------|-------|
| **Self-hosted VPS** | Technical users | `gozzip-proxy` binary on any Linux VPS ($5/month) |
| **Desktop app as proxy** | Users with always-on desktop | Tauri app includes proxy mode (toggle in settings) |
| **Community proxy** | Non-technical users | Trusted community operator runs proxy for members |
| **Relay-bundled proxy** | Herald operators | Relay operators offer proxy as a service (free or Lightning-paid) |

### Resource Requirements

The proxy is lightweight — it's essentially a light node that never sleeps:

| Resource | Requirement |
|----------|------------|
| CPU | Minimal — event forwarding, hash challenges |
| RAM | ~50-100 MB per proxied user |
| Storage | 50-200 MB per user (30-day event window + buffer) |
| Bandwidth | ~4-12 MB/day per user (light-to-full node range) |
| Network | Stable IP or domain (unlike mobile) |

A $5/month VPS can proxy 10-50 users comfortably.

### Sync Protocol Detail

When the mobile app comes online:

```
Mobile                          Proxy
  │                               │
  ├── Connect (WebSocket) ───────►│
  │                               │
  │◄── Checkpoint delta ──────────┤  (events since last sync)
  │    (Kind 10051 comparison)    │
  │                               │
  ├── ACK received events ───────►│
  │                               │
  ├── Push new own events ───────►│  (published while on mobile data)
  │                               │◄── Forward to pact partners
  │                               │
  │◄── Pact health report ────────┤  (challenge results, reliability scores)
  │                               │
  ├── Disconnect ────────────────►│
  │                               │
  │   (mobile goes to background) │
  │                               │
  │◄── Push notification ─────────┤  (DM received, high-priority event)
  │                               │
  ├── Wake + Connect ────────────►│
  │◄── Deliver buffered events ───┤
  │                               │
```

### Why Not Just Use Relays?

In the mobile-first model (Section 3), relays ARE the message bus for mobile-to-mobile pacts. The proxy adds value beyond relays:

1. **Instant challenge response** — proxy answers in milliseconds vs waiting for phone to wake (hours)
2. **Higher reliability score** — 95% uptime vs 30%, earning better pact partners
3. **Pre-aggregated sync** — proxy collects events from all pact partners, phone gets one batch
4. **More pact capacity** — popular users can handle 40+ pacts (vs 20 on mobile alone)
5. **Your infrastructure** — under your control, not dependent on third-party relays

The proxy is **your always-on node**. Mobile-only works fine for casual users; the proxy is for users who want maximum reliability and reach.

---

## 5. Node Discovery and the Dynamic IP Problem

### The Problem

Nodes need to find each other's current network address to exchange events, but:
- Mobile devices change IP every time they switch between WiFi and cellular
- Home connections have dynamic IPs that change periodically
- NAT means most devices don't have publicly routable addresses
- VPN usage adds another layer of address instability

### How Gozzip Solves This (4 Layers)

#### Layer 1: Storage Endpoint Hints (Kind 10059)

The primary discovery mechanism. When you follow someone, they send you an encrypted message containing their pact partners' connection endpoints. Wrapped in NIP-59 gift wrap for privacy — the relay stores an opaque blob, only the intended recipient can decrypt it.

```json
// Outer event (NIP-59 gift wrap — relay sees only this)
{
  "kind": 1059,
  "pubkey": "<random_throwaway_key>",
  "content": "<NIP-44 encrypted inner event>",
  "tags": [["p", "<follower_root_pubkey>"]]
}

// Inner event (only follower can decrypt)
{
  "kind": 10059,
  "content": "<NIP-44 encrypted: {\"peers\": [\"wss://relay1\", \"wss://relay2\"]}>",
  "tags": [
    ["p", "<follower_root_pubkey>"],
    ["root_identity", "<root_pubkey>"]
  ]
}
```

- **NIP-59 gift wrapping** — relay stores opaque event, can't see endpoints or identify the relationship. No relay-side filtering needed.
- Encrypted to the specific follower — only they can read the endpoints
- Updated when storage peers change (pact formed/dropped)
- WoT-scoped: only sent to followers within 2 hops (limits topology leakage)
- Followers cache these locally — no need to discover on every request
- **Works with any standard Nostr relay** — relay just stores a gift-wrapped event like any other

**In a mobile-first network (90%+ mobile), most pact partners are also mobile.** Kind 10059 endpoints will typically contain relay URLs rather than direct peer IPs:

```json
{"peers": ["wss://relay.example.com", "wss://relay2.example.com"]}
```

This tells followers: "to reach my pact partners, connect to these relays." The relays serve as stable rendezvous points for mobile-to-mobile communication. The endpoint hint shifts from "here's my partner's IP" to "here's where my partners check in."

**For the minority with stable infrastructure** (desktop full nodes, proxy daemons), endpoints can include direct addresses alongside relay URLs.

#### Layer 2: Pseudonymous Gossip Discovery (Kind 10057)

When you need data from someone whose endpoint you don't have cached, you broadcast a pseudonymous request through the gossip network using a rotating request token:

```json
{
  "kind": 10057,
  "tags": [
    ["bp", "<H(target_pubkey || 2026-03-06)>"],
    ["since", "<timestamp>"],
    ["ttl", "3"],
    ["request_id", "<unique_id>"]
  ]
}
```

- `bp` is a rotating request token — computed as H(target_pubkey || YYYY-MM-DD), a daily-rotating lookup key that prevents casual cross-day linkage but is reversible by any party knowing the target's public key. Not a formal cryptographic blinding scheme.
- Propagates through WoT (2-hop boundary, rate-limited) — **client-side forwarding, not relay logic**
- Storage peers (other users' clients) match the token against their stored pubkeys
- Responders reply privately via Kind 10058 with a connection endpoint
- **Works with any standard Nostr relay** — the relay just stores and forwards the event

**This doesn't require knowing anyone's IP.** The request travels through existing WebSocket connections between peers. Each peer's **client** that receives the request computes `H(stored_pubkey || date)` for pubkeys it stores. If match, it responds privately with its current address. The relay doesn't need to understand the token — storage peers do.

Gossip reach per request: ~400-900 unique online nodes (realistic 5K network). Since a target has ~20 pact partners, at least one is very likely to be online and reachable within 2 hops.

#### Layer 3: Relay Rendezvous (Fallback)

Relays serve as stable rendezvous points when direct peer discovery fails:

- Users publish relay preferences via NIP-65 (relay list)
- Relays have stable domains/IPs
- Kind 10058 (data offer) responses can route through relays
- Relay usage decays from 16.9% to 0.2% as pact network matures

Relays are the "DNS of last resort" — always available, rarely needed.

#### Layer 4: iroh Transport Layer

iroh decouples identity from transport address entirely:

- **A user's root pubkey IS their iroh network address** — no IP needed
- iroh mesh protocol routes datagrams by pubkey, not IP
- Spanning tree routing + bloom filter reachability handles topology changes
- Works over UDP, BLE, radio, Tor — any transport
- Sessions survive route changes (Noise XK binds to identity, not address)

**With iroh, the dynamic IP problem disappears entirely.** You address messages to a pubkey; the mesh figures out how to deliver them.

### Discovery by Platform

| Platform | Primary Discovery | Fallback | IP Stability |
|----------|------------------|----------|-------------|
| Desktop (Full) | Direct — has stable IP/domain | Relay | High |
| Extension (Light) | Relay rendezvous + cached endpoints | Gossip | N/A (outbound only) |
| Proxy Daemon | Direct — VPS with static IP | Relay | Very high |
| Mobile (no proxy) | Relay rendezvous | Gossip → relay | N/A (outbound only) |
| Mobile (with proxy) | Connects to proxy (stable IP) | Relay | Proxy is stable |

### Key Insight: Outbound-Only + Relay Rendezvous

In a 90%+ mobile network, **most nodes connect outbound to shared relays.** No node needs to be directly addressable. The relay is the meeting point.

```
Phone A (dynamic IP) ──WS──► Relay (stable domain) ◄──WS── Phone B (dynamic IP)
Extension            ──WS──► Relay (stable domain) ◄──WS── Phone C (dynamic IP)
Desktop (stable IP)  ──WS──► Relay (stable domain)  (also accepts direct connections)
```

**No phone needs a stable IP.** No phone needs to be findable. Every phone connects outbound to relays it trusts. Pact messages flow through those relays.

The relay's role is minimal: store events and serve subscriptions — exactly what standard Nostr relays already do. No custom code, no modifications, no Gozzip-specific logic. Multiple relays provide redundancy. Unreliable relays get dropped via NIP-65.

### NAT Traversal

**Mostly unnecessary** in the relay-mediated model — both sides connect outbound to relays, sidestepping NAT entirely. For direct connections when desired:

1. **WebSocket via relay** — the default, works always
2. **iroh hole punching** (future) — UDP hole punching coordinated via mutual peers
3. **BLE direct** (mobile, iroh Tier 0) — Bluetooth discovery via geohash-tagged ephemeral keys, no IP needed
4. **WebRTC** (potential) — STUN/TURN for browser-to-mobile direct connections when both are online

### Address Update Protocol

For mobile (90%+ of users): **address changes don't matter.** Phones connect outbound to relays. The relay URL is the stable address, not the phone's IP.

For full nodes/desktops with incoming connections:
1. Node detects IP change (OS network event)
2. Updates DNS (DDNS) or publishes updated Kind 10059 to followers
3. Reconnects outbound WebSockets to relays and pact partners

**Frequency:** VPS: almost never. Desktops: occasional (DHCP renewal). Mobile: irrelevant.

---

## 6. Capability Matrix

| Capability | Desktop | Extension | Mobile (no proxy) | Mobile (with proxy) |
|-----------|---------|-----------|-------------------|---------------------|
| Full node (Keeper) | Yes | No | No | No |
| Light node (Witness) | Yes | Yes | Yes | Yes |
| Pact storage | Yes | Yes (IndexedDB) | Yes (on-device) | Yes (on-device + proxy) |
| Challenge response | Yes | Yes (service worker) | Async via relay (hours) | Instant via proxy |
| Gossip forwarding | Yes | Yes | When online | Proxy when offline |
| Feed model (3-tier) | Yes | Yes | Yes | Yes |
| Event publishing | Yes | Yes | Yes | Yes |
| NIP-07 signing | N/A | Yes (primary use) | N/A | N/A |
| BLE mesh (iroh) | Limited | No | Yes (Android > iOS) | Yes |
| Push notifications | N/A | N/A | Yes (APNS/FCM) | No |
| Offline operation | Yes (desktop on) | No (browser must run) | Via proxy | No |

---

## 7. Technology Stack

### Shared Rust Core

All platforms share the same Rust protocol core:

```
gozzip-core (Rust library)
├── pact/           Pact formation, challenge-response, lifecycle
├── gossip/         WoT-filtered gossip routing
├── feed/           3-tier feed model (Inner Circle, Orbit, Horizon)
├── interaction/    InteractionTracker, scoring, referrals
├── crypto/         secp256k1 signing, checkpoint merkle, rotating request tokens
├── sync/           Checkpoint reconciliation, batch sync
├── storage/        Event storage, LRU cache, eviction
└── types/          Event, Pact, Checkpoint, WotTier, etc.

Compilation targets:
├── Native (x86_64, aarch64)  → Desktop (Tauri) + Pact Proxy daemon
├── WASM (wasm32)             → Browser extension
├── iOS (aarch64-apple-ios)   → Mobile via UniFFI → Swift
└── Android (aarch64-linux-android) → Mobile via UniFFI → Kotlin
```

### Per-Platform UI

| Platform | UI Framework | Integration |
|----------|-------------|-------------|
| Desktop | Tauri 2.0 (web frontend) | Rust backend direct |
| Extension | Chrome/Firefox/Safari popup + content script | WASM in service worker |
| Android | Jetpack Compose | UniFFI → Kotlin bindings |
| iOS | SwiftUI | UniFFI → Swift bindings |

### SDK Ecosystem

| SDK | Use Case |
|-----|----------|
| rust-nostr (v0.44+) | Nostr event types, NIP implementations, already in simulator |
| nostr-tools (JS) | Web app integration, NIP-07 client side |
| NDK (JS) | React/Svelte web app with outbox model |
| NDK (Dart) | Flutter mobile option |
| @noble/secp256k1 | JS fallback for extension crypto (if WASM unavailable) |

---

## 8. Risk Assessment

### High Risk

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Chrome changes MV3 service worker lifecycle | Extension can't maintain pact obligations | Offscreen document fallback; Firefox event pages as backup platform |
| iOS tightens background restrictions further | Mobile-to-mobile pact sync degrades | Relay-mediated async model already handles this; proxy optional |
| WASM instantiation too slow on worker restart | Gossip latency spikes after idle | Keep WASM binary small (<1 MB); cache compiled module in IndexedDB |
| Relay dependency in mobile-first network | 90%+ mobile means relays become critical path for pact messages | Multiple relay redundancy; relay is dumb pipe (no custody); iroh eliminates relay need long-term |

### Medium Risk

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Extension store rejection (Chrome Web Store) | Delayed distribution | Ship on Firefox first (less restrictive); offer direct download |
| Cross-browser service worker differences | Maintenance burden | Abstraction layer over background context; test matrix |
| Pact Proxy becomes a centralizing force | Undermines protocol goals | Make proxy self-hostable; desktop app can proxy for own mobile |

### Low Risk

| Risk | Impact | Mitigation |
|------|--------|-----------|
| secp256k1 WASM performance | Signing latency | Well-benchmarked; sub-millisecond in practice |
| IndexedDB storage limits | Can't store enough events | Chrome allows 60% of disk; more than sufficient |
| UniFFI binding complexity | Mobile build friction | Battle-tested by Mozilla (Firefox) and rust-nostr |

---

## 9. Implementation Roadmap

### Phase 1: Browser Extension

Extract protocol core from simulator into `gozzip-core` Rust library. Compile to WASM. Build Chrome extension with:
- NIP-07 signer (compatible with all Nostr web apps immediately)
- Light node: form pacts, store events, answer challenges
- Service worker with WebSocket connections to pact partners
- IndexedDB for event storage and protocol state
- Basic popup UI showing pact status, feed, WoT tier info

**Deliverable:** Chrome extension that turns any Nostr web app into a Gozzip client.

### Phase 2: Desktop App (overlaps with Phase 1)

Tauri 2.0 app using the same `gozzip-core` library (native, not WASM). Full node capable:
- Always-on pact storage with full history
- Gossip routing hub
- Can act as Pact Proxy for the user's own mobile device
- Web-based UI (shared with extension where possible)

**Deliverable:** Desktop app that serves as a full node and mobile proxy.

### Phase 3: Pact Proxy Daemon

Headless Rust daemon (same `gozzip-core`) for VPS deployment:
- Answers challenges on behalf of mobile nodes
- Buffers events for mobile sync
- Push notification gateway (APNS/FCM)
- Minimal resource footprint

**Deliverable:** `gozzip-proxy` binary deployable on any Linux VPS.

### Phase 4: Android App

UniFFI bindings from `gozzip-core` → Kotlin. Jetpack Compose UI:
- Light node with ForegroundService for background connectivity
- Syncs with Pact Proxy when backgrounded
- BLE mesh support (iroh Tier 0)
- FCM push notifications

### Phase 5: iOS App

UniFFI bindings → Swift. SwiftUI:
- Light node, foreground-only pact participation
- BGAppRefreshTask for periodic sync (~30s windows)
- Full delegation to Pact Proxy when backgrounded
- APNS push notifications
- CoreBluetooth for limited BLE mesh

---

## Appendix A: Existing Nostr Client Landscape

### Mobile Clients

| Client | Platform | Stack | Architecture |
|--------|----------|-------|-------------|
| Damus | iOS | Swift, SwiftUI | Direct relay connections, Notepush for push notifications |
| Nostur | iOS/macOS | Swift, SwiftUI | Relay Autopilot, VPN detection, WoT spam filtering |
| Amethyst | Android | Kotlin, Compose | Quartz (KMP) shared lib, never closes relay connections |
| Primal | iOS/Android/Web | Native per platform | Caching server aggregates all relays; single WS to client |

### Desktop Clients

| Client | Stack | Notable |
|--------|-------|---------|
| Gossip | Rust, LMDB, egui | Gossip model pioneer — connects to minimum relay set via NIP-65 |
| Nostur | Swift, SwiftUI (macOS) | Same codebase as iOS |

### Web Clients

| Client | Stack | Notable |
|--------|-------|---------|
| Snort | React, TypeScript | Fast, feature-packed |
| Coracle | Svelte 4, TypeScript | Privacy-focused, WoT moderation |
| Iris | Svelte SPA | Minimalist, built-in Cashu wallet |
| Primal | Web app | Backed by caching server |

### NIP-07 Extensions

| Extension | Browser | Notes |
|-----------|---------|-------|
| nos2x | Chrome | Original NIP-07 signer by fiatjaf |
| nos2x-fox | Firefox | nos2x fork |
| Alby | Chrome, Firefox | Lightning wallet + NIP-07 |
| nostr-keyx | Chrome | OS keychain / YubiKey key storage |

### Mobile P2P Precedents

| App | Approach | iOS? |
|-----|----------|------|
| Briar | Pure P2P (Bluetooth/Tor) | **No, never** — iOS kills background Bluetooth |
| Session | Onion routing + Service Nodes | Yes — FCM/polling hybrid for background |
| Element | Matrix homeservers | Yes — push gateway for notifications |

Every successful decentralized mobile messenger converges on: **centralized push (APNS/FCM) as wake-up signal, decentralized operations in foreground windows.**

## Appendix B: Nostr SDK Ecosystem

| SDK | Language | Mobile | Maturity |
|-----|----------|--------|----------|
| rust-nostr | Rust + UniFFI → Swift, Kotlin, JS | Yes | Alpha |
| nostr-tools | JavaScript | Via JS runtime | Production |
| NDK (JS) | TypeScript | React Native first-class | Production |
| NDK (Dart) | Dart | Flutter native | Active |
| Quartz | Kotlin Multiplatform | Android native, iOS planned | Active |
