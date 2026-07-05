## Context

Builds on `email-core`. Message view and compose are complete. This change adds a PGP layer: detection at open time, crypto in-browser via OpenPGP.js, key storage on server as opaque blobs. S/MIME is deferred (X.509 CA chain complexity).

## Decisions

### D1: Client-side crypto — OpenPGP.js (WebCrypto-backed)

All PGP operations (sign, verify, encrypt, decrypt) run in the browser using OpenPGP.js. Private keys never transmitted to or processed by the server in plaintext. OpenPGP.js uses the browser's WebCrypto API for performance.

Bundle impact: ~600 KB gzipped. Mitigation: lazy-load module only when a PGP key is configured or a PGP message is detected. Not in initial bundle.

### D2: Key storage — hybrid model (encrypted blob on server)

Private key encrypted client-side with a key derived from a user-supplied passphrase (Argon2id, 256-bit salt). Ciphertext blob uploaded to server. On any device: download blob → enter passphrase → decrypt key in browser → cache unlocked key in `sessionStorage` (cleared on tab close). Server holds an opaque blob it cannot decrypt.

Key schema (per-user mail.db):
```
pgp_keys:     id, fingerprint, uid, public_key_armored,
              private_key_encrypted_blob BLOB NOT NULL, is_primary, created_at
contact_keys: id, email, public_key_data,
              source (manual|wkd|keyserver|received), fingerprint, fetched_at
```

### D3: Public key discovery — privacy-ordered

1. `contact_keys` table (previously fetched/imported) — no external contact
2. Scan previously received signed mail from that sender — no external contact
3. Manual import (armored public key paste/upload) — no external contact
4. **WKD** (`GET https://<domain>/.well-known/openpgpkey/hu/<hash>`) — contacts recipient's own domain only; **opt-in**, off by default
5. **HKP keyserver** (`keys.openpgp.org`) — contacts central keyserver, reveals queried email to operator; **opt-in**, off by default, separate toggle from WKD

Discovery controlled by `pgp_discovery_wkd_enabled` and `pgp_discovery_keyserver_enabled` user settings (both default `false`). When both disabled, compose shows "Key discovery disabled — import key manually" if no key found locally.

### D4: Inbound handling

Backend stores encrypted body as-is in blob storage — server cannot decrypt PGP content; the blob is opaque ciphertext. Frontend fetches blob, detects `multipart/encrypted` (PGP/MIME) or `-----BEGIN PGP MESSAGE-----` (inline PGP) or `multipart/signed`, lazy-loads OpenPGP.js, decrypts/verifies. Signature verification checks `contact_keys` for sender's public key.

### D5: Outbound compose

Sign and/or encrypt toggles in compose toolbar. Per-account default (`sign_by_default` on `email_accounts`). Missing recipient key → warning banner, send blocked for encryption (user must resolve). Sign-then-encrypt ordering enforced (standard PGP convention). PGP/MIME wrapping for both sign and encrypt. Pre-enable Sign + Encrypt when replying to an encrypted message.
