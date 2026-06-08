## ADDED Requirements

### Requirement: PGP key generation
The system SHALL allow authenticated users to generate an OpenPGP key pair in the browser. The private key SHALL be encrypted with a user-supplied passphrase using Argon2id key derivation before leaving the browser. All crypto operations SHALL use OpenPGP.js running in the browser; the server SHALL never receive an unencrypted private key.

#### Scenario: Generate key pair
- **WHEN** a user generates a new PGP key with a name, email, and passphrase
- **THEN** OpenPGP.js generates the key pair in browser, the private key is encrypted with the passphrase-derived key, the encrypted blob is uploaded to `POST /pgp-keys`, and the public key is stored in armored form

#### Scenario: Passphrase too weak
- **WHEN** a user submits a passphrase shorter than 12 characters
- **THEN** the frontend rejects it before generation with a strength error

---

### Requirement: PGP key import and export
The system SHALL allow users to import existing PGP private keys (armored) and export their public or private keys.

#### Scenario: Import private key
- **WHEN** a user pastes or uploads an armored private key and supplies the passphrase
- **THEN** the key is re-encrypted with the Argon2id-derived passphrase, the blob is stored on the server, and the key is unlocked into `sessionStorage`

#### Scenario: Export public key
- **WHEN** a user exports their public key
- **THEN** the browser downloads the armored public key (no passphrase required)

#### Scenario: Export private key
- **WHEN** a user exports their private key
- **THEN** the browser downloads the armored private key encrypted with the existing passphrase, after confirming passphrase entry

---

### Requirement: Key unlock and session cache
The system SHALL prompt for the PGP passphrase when a crypto operation is needed and no unlocked key is in `sessionStorage`. The unlocked key SHALL be cached in `sessionStorage` for the browser session and cleared on tab close.

#### Scenario: Unlock key for session
- **WHEN** a crypto operation is needed and no key is unlocked
- **THEN** a passphrase prompt is shown; on correct entry the key is decrypted and cached in `sessionStorage`

#### Scenario: Wrong passphrase
- **WHEN** the user enters an incorrect passphrase
- **THEN** the unlock fails, no key is cached, and an error is shown

---

### Requirement: Public key discovery for outbound encryption
The system SHALL search local sources first: (1) `contact_keys` cache, (2) previously received signed mail from that sender. External lookups (WKD, HKP keyserver) SHALL only run if explicitly enabled by the user (`pgp_discovery_wkd_enabled`, `pgp_discovery_keyserver_enabled`), both defaulting to `false`. When external lookups are disabled and no local key exists, the system SHALL inform the user to import the key manually. External lookups are proxied via backend to avoid CORS and to cache results.

#### Scenario: Key found in local cache
- **WHEN** the recipient's key is in `contact_keys`
- **THEN** encryption is available; no external request is made

#### Scenario: External lookups disabled, no local key
- **WHEN** no local key exists and both discovery settings are `false`
- **THEN** lock icon absent; compose shows "No key found — import manually or enable key discovery in settings"

#### Scenario: WKD lookup enabled and key found
- **WHEN** `pgp_discovery_wkd_enabled=true` and a WKD key exists at the recipient's domain
- **THEN** backend fetches key (contacts recipient's domain only), caches in `contact_keys`, encryption available

#### Scenario: Keyserver lookup enabled and key found
- **WHEN** `pgp_discovery_keyserver_enabled=true` and `keys.openpgp.org` has a matching key
- **THEN** key fetched via backend proxy, stored with `source=keyserver`, encryption available

#### Scenario: No key found through any enabled method
- **WHEN** no key found after all enabled discovery steps
- **THEN** encryption unavailable; send blocked if encrypt toggle is forced on

---

### Requirement: Manual public key import for contacts
The system SHALL allow users to manually import a contact's armored public key, stored in `contact_keys`.

#### Scenario: Import contact key
- **WHEN** a user pastes an armored public key for a contact email address
- **THEN** it is stored in `contact_keys` with `source=manual` and is immediately available for encryption

---

### Requirement: Decrypt inbound encrypted messages
The system SHALL detect and decrypt PGP-encrypted messages (PGP/MIME `multipart/encrypted` and inline PGP armor) in the browser using OpenPGP.js. The backend stores the encrypted body unmodified; decryption happens client-side only.

#### Scenario: Decrypt PGP/MIME message
- **WHEN** a user opens a message with `Content-Type: multipart/encrypted; protocol="application/pgp-encrypted"`
- **THEN** the frontend prompts for the passphrase if needed, decrypts the body with OpenPGP.js, and displays the plaintext

#### Scenario: Decryption key not available
- **WHEN** the message was encrypted to a key the user does not hold
- **THEN** the frontend displays "Cannot decrypt — no matching private key" without attempting decryption

#### Scenario: Decryption failure
- **WHEN** decryption fails due to corrupted data or wrong key
- **THEN** an error is displayed with the raw encrypted body accessible via a "Show raw" toggle

---

### Requirement: Verify inbound signed messages
The system SHALL verify PGP signatures on `multipart/signed` messages and inline-signed messages. Verification result SHALL be displayed as a badge: verified (known key), unverified (key not in contact_keys), or invalid (bad signature).

#### Scenario: Valid signature from known sender
- **WHEN** a message is signed and the sender's public key is in `contact_keys`
- **THEN** a green "Verified" badge is shown with key fingerprint

#### Scenario: Signature present but key unknown
- **WHEN** a message is signed but the sender's key is not in `contact_keys`
- **THEN** a yellow "Unverified — key unknown" badge is shown; the system attempts WKD/keyserver discovery automatically

#### Scenario: Invalid signature
- **WHEN** the signature does not match the message content
- **THEN** a red "Invalid signature" badge is shown

---

### Requirement: Sign outbound messages
The system SHALL allow users to sign outgoing messages using their PGP private key (PGP/MIME detached signature). Signing SHALL be opt-in per message with a persistent default preference per account.

#### Scenario: Sign message
- **WHEN** a user enables the Sign toggle and sends
- **THEN** the message is sent as `multipart/signed` with a detached PGP signature

#### Scenario: Sign default preference
- **WHEN** a user enables "Always sign" in account settings
- **THEN** all outgoing messages from that account have Sign pre-enabled in compose

---

### Requirement: Encrypt outbound messages
The system SHALL allow users to encrypt outgoing messages to all recipients' public keys. Encryption SHALL be blocked if any recipient has no available public key, with a clear warning identifying the missing key(s).

#### Scenario: Encrypt message to all recipients
- **WHEN** the Encrypt toggle is enabled and all recipients have known keys
- **THEN** the message is sent as `multipart/encrypted` (PGP/MIME), encrypted to all recipient keys and the sender's own key

#### Scenario: Encrypt blocked — missing key
- **WHEN** the Encrypt toggle is enabled but one or more recipients have no key
- **THEN** send is blocked and a banner lists the recipients missing keys, with options to search keyservers or import manually

#### Scenario: Reply to encrypted thread
- **WHEN** a user replies to an encrypted message
- **THEN** Encrypt and Sign are pre-enabled by default

---

### Requirement: Key deletion
The system SHALL allow users to delete their PGP keys. Deleting a key SHALL remove the blob from the server and clear it from `sessionStorage`.

#### Scenario: Delete key
- **WHEN** a user deletes a PGP key
- **THEN** the key blob is removed from the server; any messages encrypted only to that key become undecryptable (user warned before deletion)
