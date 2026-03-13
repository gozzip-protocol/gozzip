# Key Management UX

**Date:** 2026-03-14
**Status:** Draft
**Addresses:** Agent 07 (Mobile Constraints) §3, §7; Agent 10 (Practical Deployment) §6

## Problem Statement

The protocol uses a 4-level key hierarchy (root, governance, DM, device subkeys). This provides excellent cryptographic compartmentalization — device compromise is contained, root key lives in cold storage, DM keys rotate independently.

However, this creates a UX problem: users must understand key types, manage cold storage, access root keys for device onboarding and DM rotation, and configure social recovery. No mainstream messaging app requires any of this.

The design principle from the vision doc is clear: "If a feature requires the user to understand cryptography, the design is wrong."

## Design Goals

1. **Zero key management for standard users.** The app generates, stores, and rotates keys automatically. Users never see a key, a hex string, or a "cold storage" prompt.
2. **Progressive disclosure for power users.** Advanced options (hardware wallet, air-gapped signing, manual key export) are available but hidden by default.
3. **Device onboarding without cold storage.** Adding a new phone should be as easy as scanning a QR code from an existing device.
4. **Automatic DM key rotation.** No user action required, ever.

## Standard Mode (Default)

### First Launch (New Identity)

1. App generates root keypair
2. Root key encrypted with device biometric (Face ID / Touch ID / fingerprint)
3. Root key backed up to platform secure storage:
   - **iOS:** iCloud Keychain with `kSecAttrSynchronizable = true`, protected by device passcode + Apple ID
   - **Android:** Google Cloud Keystore / Backup Key Vault, protected by device lock screen
   - **Desktop:** OS keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service)
4. Governance key derived and stored in platform secure storage
5. Device subkey derived and published (kind 10050)
6. DM key derived and published
7. User sees: "Your account is ready. Your identity is secured by [Face ID / fingerprint / device PIN]."

**No mention of:** root keys, cold storage, key hierarchy, hex strings, or seed phrases.

### Adding a New Device

**Current problem:** Requires root key access to publish updated kind 10050 with new device delegation.

**Proposed solution: Device-to-device delegation**

1. User opens "Add Device" in existing (authorized) device
2. Existing device generates a QR code containing:
   - A one-time pairing token
   - Encrypted connection details (relay for handshake)
3. New device scans QR code
4. Devices establish NIP-46 encrypted channel via relay
5. Existing device signs a **temporary delegation** for the new device (valid 7 days)
6. New device operates immediately with the temporary delegation
7. Within 7 days, the root key (available via iCloud Keychain / Google Backup) confirms the delegation by publishing an updated kind 10050
8. If root key confirmation fails (e.g., backup not accessible), the temporary delegation expires and the new device loses access

**UX flow:** Scan QR code → "New device linked. Confirm within 7 days." → Background confirmation happens automatically via synced keychain.

**Security:** A compromised existing device can grant 7-day temporary access. This is bounded damage — the temporary key has limited capabilities and the root key (in cloud keychain) must confirm. The 7-day window is a configurable parameter.

### DM Key Rotation

**Clarification:** DM key derivation uses `KDF(root, "dm-decryption-" || rotation_epoch)`. This requires the root private key.

**Solution:** The root key is available on-device (encrypted by biometric in standard mode), so rotation is automatic:

1. Every 90 days, the client derives a new DM key from the root key
2. Publishes updated kind 10050 with new DM public key
3. Maintains decryption for both old and new DM keys for a 7-day overlap window
4. After overlap, old DM key material is securely deleted via platform API:
   - iOS: `SecItemDelete` with Keychain Services
   - Android: `KeyStore.deleteEntry()`
5. User sees: nothing. Rotation is invisible.

### Social Recovery Setup

During onboarding (after the user has followed 3+ contacts):

1. App prompts: "Choose 3 trusted contacts who can help recover your account if you lose all your devices."
2. User selects 3-5 contacts from their follow list
3. App publishes kind 10060 (recovery delegation) with NIP-44 encrypted shares
4. User sees: "Account recovery set up. [Alice, Bob, Carol] can help you recover your identity."

**No mention of:** N-of-M threshold, Shamir's secret sharing, timelocks, or key shards.

### Key Loss Scenarios

| Scenario | What happens | User action |
|----------|-------------|-------------|
| Lost phone | Root key in iCloud/Google backup. New device restores automatically. | Set up new phone, sign in to Apple/Google account |
| Lost phone + no cloud backup | Social recovery (kind 10060/10061). 3-of-5 contacts attest. 7-day timelock. | Contact recovery friends, wait 7 days |
| Lost all devices + cloud backup | Restore from iCloud/Google. Root key recovered. | Sign in on new device |
| Lost all devices + no cloud + no recovery | Identity is permanently lost | — |

## Advanced Mode (Opt-In)

Available under Settings → Security → Advanced Key Management:

### Hardware Wallet Storage

1. User connects hardware wallet (Ledger, Trezor, or compatible)
2. Root key exported to hardware wallet
3. On-device root key securely deleted
4. All operations requiring root key (device delegation, DM rotation) prompt hardware wallet connection
5. User understands trade-off: maximum security, less convenience

### Manual Key Export

1. User can export root key as encrypted file or BIP-39 mnemonic
2. Strong warning: "This is your master key. Anyone with this key controls your identity."
3. Export protected by additional PIN/passphrase

### Governance Key Placement

For high-security users:
1. Governance key can be moved to a separate device
2. Profile and follow-list changes require the governance device
3. Day-to-day posting uses device subkeys only

## Revocation

### Standard Mode

1. User reports device lost/stolen in app (from another authorized device)
2. App publishes updated kind 10050 revoking the compromised device
3. If no other authorized device exists: social recovery path

### Emergency Revocation

If the user suspects compromise but doesn't have immediate root key access:

1. Governance key (available on the user's primary device) publishes a kind 10065 "temporary suspension" event
2. Clients treat the suspended device as revoked immediately
3. Root key must confirm (permanent revocation) or cancel within 72 hours
4. If neither happens, suspension lapses (prevents governance key compromise from permanently revoking devices)

## Interaction with Protocol

| Key operation | Standard mode | Advanced mode |
|--------------|---------------|---------------|
| Root key storage | Device biometric + cloud backup | Hardware wallet |
| Device onboarding | QR code scan + 7-day temp delegation | Hardware wallet required |
| DM key rotation | Automatic (90 days) | Hardware wallet prompt |
| Profile changes | Governance key on-device | Governance key on separate device |
| Social recovery | 3-of-5 contacts | Configurable N-of-M |
| Revocation | From any authorized device | Hardware wallet required |

## Implementation Priority

1. **Standard mode** — must ship with first client (pre-launch requirement)
2. **Social recovery** — must ship with first client (Agent 10: "root loss is fatal without this")
3. **Device-to-device delegation** — ship within 3 months of launch
4. **Emergency revocation** — ship within 3 months of launch
5. **Advanced mode** — ship when power users request it
