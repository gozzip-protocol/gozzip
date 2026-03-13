# Adversarial Privacy Analysis of the Gozzip Protocol

**Date:** 2026-03-12
**Reviewer:** Privacy Research Agent (adversarial)
**Status:** Complete
**Threat model:** Well-resourced nation-state adversary (NSA/GCHQ-class) as worst case; commercial surveillance (relay operators, ISPs) as baseline

---

## Executive Summary

Gozzip makes careful, generally honest privacy claims -- the surveillance-surface.md document in particular is commendably transparent about what the protocol does and does not hide. However, several claims are framed more favorably than the underlying cryptographic and network-level realities warrant. The protocol provides meaningful privacy improvements over vanilla Nostr but falls fundamentally short of the privacy guarantees offered by Tor, Signal, Briar, or Session.

---

## Issue 1: Metadata Leakage via Relay Patterns

### The claim

surveillance-surface.md states: "No single point of full visibility. No relay, ISP, or peer sees the complete picture." The summary table shows a single relay operator as seeing only a "Partial (read patterns)" social graph. The whitepaper (Section 7, Table: Content Distribution) says Gozzip provides "Blinded pubkeys rotate daily; storage peers cannot identify the requester."

### Actual guarantee

A single relay operator, over time, can reconstruct a substantial portion of the social graph for users who use that relay. The operator sees:

1. **Which pubkeys publish to them.** This is a complete set of writers -- there is no blinding for publishing. Every event carries the author's device pubkey and a `root_identity` tag in the clear.

2. **Which clients fetch which pubkeys.** Even without blinded requests, standard Nostr REQ subscriptions filter by author pubkey. The relay sees "client at IP X subscribed to events from pubkey Y." Over a week, the relay builds the follow list for every client that connects.

3. **Timing correlations.** When Alice publishes and Bob fetches within seconds, the relay has strong evidence of an active follow relationship.

4. **Kind 10050 lookups.** When a client fetches the device delegation for a root pubkey, the relay learns "this client is interested in this identity." This is an *additional* signal beyond regular subscriptions.

5. **Kind 10059 delivery.** While gift-wrapped, the relay knows the `p` tag (the follower pubkey) on the outer NIP-59 wrapper. The relay sees "user A sent something to user B" even if the content is encrypted.

The claim of "partial" graph visibility is technically true -- one relay does not see 100% of a user's follows if the user spreads subscriptions across relays. But in practice, if a user connects to 3-5 relays and each relay sees the subscriptions made through it, each relay sees a fraction. However, the *most popular* relays see disproportionately large fractions. If Relay A is used by 60% of the network, Relay A's operator sees 60% of all follow relationships that route through them. The "partial" framing understates how complete this partial view typically is.

**Quantitative estimate:** If a user follows 200 accounts and uses 3 relays following NIP-65 outbox model, each relay sees the subscriptions for authors whose outbox includes that relay. In practice, popular relays serve as outbox for a large fraction of authors. A relay used by 50% of authors as an outbox relay would see approximately 100 of 200 follow subscriptions for any given client -- enough to reconstruct the majority of the social graph.

### Gap

The documentation says "partial" as if this is a strong limitation. In realistic relay distributions, "partial" means "50-80% of the graph" for popular relays. This is not an honest framing for users evaluating the protocol's privacy.

### Severity: Significant

This is not a protocol bug -- it is a structural property of the outbox model. But it is understated.

### Suggested fix

Quantify "partial." State explicitly: "A relay operator serving as outbox for N% of authors in a user's follow list can reconstruct approximately N% of that user's follow graph from subscription patterns. In concentrated relay ecosystems, this could be 50-80% of the graph." Add a recommendation for minimum relay diversity (e.g., "use at least 5 relays with no single relay covering more than 30% of your follows").

---

## Issue 4: BLE Privacy Claims

### The claim

bitchat-integration.md states: "Ephemeral subkeys prevent linking presence to persistent identity." surveillance-surface.md states that a BLE scanner sees "encrypted Noise Protocol sessions between peers" and does NOT see "Identity (ephemeral subkeys for nearby mode, not linked to root pubkey)."

### Actual guarantee

The ephemeral key design for nearby mode is genuinely strong -- keys are not derived from the root key, not listed in kind 10050, and discarded on deactivation. However, the BLE layer has several privacy limitations that the documentation does not address:

1. **BLE MAC address rotation.** BLE devices advertise using a MAC address. Modern devices use random resolvable private addresses (RPAs) that rotate periodically (typically every 15 minutes per the Bluetooth spec). However:
   - The rotation period is device-dependent, not protocol-controlled.
   - During the rotation period, the MAC address is a stable identifier.
   - Some devices (especially older Android versions) do not rotate properly.
   - The RPA resolution key (IRK) stored in bonded devices can be used to track rotations.

2. **BLE physical-layer fingerprinting.** Research has demonstrated that BLE transmitters have unique RF fingerprints based on manufacturing imperfections in the radio hardware (clock drift, carrier frequency offset, I/Q imbalance). A sophisticated attacker with SDR equipment can fingerprint individual BLE radios regardless of MAC rotation or ephemeral keys. See: "BLE-Guardian: A Lightweight Authentication Protocol for BLE IoT Devices" (2020), "Tracking Anonymized Bluetooth Devices" (2019, Boston University).

3. **Signal strength patterns.** A persistent BLE scanner at a fixed location can build a signal strength profile (RSSI pattern) for a device over time. Combined with movement patterns, this can distinguish individuals even across key rotations. If "device at this signal strength walked past every day at 8:15 AM," the pattern is identifying even without any cryptographic linkage.

4. **Session duration and data volume.** Even with ephemeral keys, a scanner can observe: "two devices established an encrypted session for 45 seconds and exchanged approximately 12KB of data." The duration and volume of sessions, combined with physical proximity patterns, can fingerprint interactions.

5. **Non-nearby BLE mesh relay.** The protocol relays Gozzip events over BLE mesh up to 7 hops. When a device relays events that are *not* in nearby mode (i.e., regular pact sync over BLE when internet is unavailable), the events carry `root_identity` tags. A BLE scanner that can observe the relayed packets (even encrypted at the Noise layer) can correlate the timing and volume of relays to infer the device's regular (non-ephemeral) activity patterns.

### Gap

The documentation claims ephemeral keys prevent identity linkage. This is true at the cryptographic layer. But BLE has well-documented physical-layer privacy problems that operate below the cryptographic layer. The documentation does not distinguish between "cryptographic unlinkability" (achieved) and "physical-layer unlinkability" (not achieved and possibly not achievable).

### Severity: Minor for most users, Significant for high-risk users (activists, journalists in physical proximity to adversaries)

The BLE privacy claims are accurate at the protocol level but incomplete at the physical level. Users relying on BLE for anonymity in adversarial physical environments (protests, conflict zones) need to understand the limitations.

### Suggested fix

1. Add a section to bitchat-integration.md: "BLE Physical-Layer Privacy Limitations" acknowledging RF fingerprinting, MAC rotation limitations, and signal strength correlation.
2. State explicitly: "Ephemeral keys provide cryptographic unlinkability between BLE sessions. However, BLE radio hardware may be fingerprinted at the physical layer, and signal strength patterns may enable tracking across key rotations. Users in adversarial physical environments should assume that persistent BLE scanning can correlate their device's presence across sessions regardless of ephemeral keys."
3. Consider recommending that high-risk users disable BLE when not actively needing mesh communication.

---

## Issue 7: Relay Fingerprinting by ISPs

### The claim

surveillance-surface.md states an ISP sees "connections to relay servers (WebSocket)" and "That this device is running a Gozzip client (unless using Tor/VPN)."

### Actual guarantee

This claim is honest but understates the ease of fingerprinting:

1. **WebSocket connections to known relay IPs.** Nostr relays operate at known IP addresses and domains. An ISP (or any network observer) can maintain a list of known relay IPs. Any WebSocket connection to these IPs identifies the user as a Nostr/Gozzip user. This is equivalent to identifying Tor users by connections to known Tor entry nodes -- a solved problem for state-level adversaries.

2. **SNI (Server Name Indication) in TLS.** Even with encrypted WebSocket (wss://), the SNI field in the TLS handshake reveals the relay domain name in cleartext (unless ECH/ESNI is used, which is not yet widely deployed). An ISP sees "user connected to relay.example.com" -- confirming both Nostr usage and which specific relay.

3. **Connection patterns distinguish Gozzip from vanilla Nostr.** A vanilla Nostr client connects to relays and subscribes. A Gozzip client additionally:
   - Maintains persistent connections (full nodes at 95% uptime)
   - Exhibits challenge-response traffic patterns (periodic kind 10054)
   - Generates gossip-forwarding traffic (kind 10055, 10057)
   - Performs NAT hole-punch signaling

   These patterns create a distinctive traffic fingerprint that could distinguish Gozzip users from vanilla Nostr users, even without decrypting content.

4. **Pact sync creates distinctive traffic patterns.** Bulk pact synchronization (syncing a partner's event history) creates bursts of data transfer to/from relay IPs that differ from normal Nostr usage (which is primarily small subscription/event exchanges). A network observer who monitors traffic volume patterns can identify pact sync operations.

### Gap

The documentation acknowledges ISP visibility of relay connections but does not discuss the fingerprinting that distinguishes Gozzip from vanilla Nostr, or the ease with which relay IPs are enumerated.

### Severity: Minor

This is a known limitation of any relay-based protocol. Tor has the same problem with entry nodes. The mitigation (use Tor/VPN) is mentioned. But the additional Gozzip-specific traffic fingerprinting should be acknowledged.

### Suggested fix

1. Acknowledge that Gozzip traffic patterns may be distinguishable from vanilla Nostr traffic by a network observer.
2. Recommend Tor or VPN not just for IP hiding but for traffic pattern obfuscation.
3. Consider adding pluggable transport support (as Tor does with obfs4) for censorship-resistance scenarios where even connecting to known relay IPs is dangerous.
4. Long-term: evaluate domain fronting or meek-style transports for relay connections in censored environments.

---

## Summary of Findings

| # | Issue | Severity | Status in Docs |
|---|-------|----------|----------------|
| 1 | Relay read-pattern graph reconstruction | Significant | Acknowledged but understated ("partial") |
| 4 | BLE physical-layer fingerprinting | Minor | Not addressed |
| 7 | ISP relay fingerprinting | Minor | Partially addressed |

## Overall Assessment

Remaining issues focus on quantifying relay graph reconstruction (Issue 1), BLE physical-layer privacy limitations not yet documented (Issue 4), and Gozzip-specific ISP traffic fingerprinting not yet acknowledged (Issue 7). The most impactful privacy documentation gaps (NIP-46 timing, NAT exposure framing, colluding-relay claims, permanent identity linkage, and privacy non-goals) have been addressed.
