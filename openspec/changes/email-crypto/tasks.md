## 1. OpenPGP — Backend

- [ ] 1.1 Implement `POST /pgp-keys` — store encrypted key blob in `pgp_keys.private_key_encrypted_blob` + public key armored; reject if uploaded data is not armored ciphertext
- [ ] 1.2 Implement `GET /pgp-keys` — return user's keys (public key + metadata; never include blob in list response)
- [ ] 1.3 Implement `GET /pgp-keys/:id/blob` — return `private_key_encrypted_blob` from DB (authenticated user only)
- [ ] 1.4 Implement `DELETE /pgp-keys/:id` — remove DB row
- [ ] 1.5 Implement `GET /keys/discover?email=<address>` — only runs WKD/HKP lookups when user settings `pgp_discovery_wkd_enabled` / `pgp_discovery_keyserver_enabled` are true; local-cache check (`contact_keys`) always runs first
- [ ] 1.6 Implement `POST /contact-keys` — manually store a contact's public key
- [ ] 1.7 Implement `GET /contact-keys?email=<address>` — lookup cached contact key
- [ ] 1.8 Add `pgp_key_id` (nullable FK to `pgp_keys`) and `sign_by_default` (bool) to `email_accounts` table

## 2. OpenPGP — Frontend

- [ ] 2.1 Add `openpgp` npm package; configure lazy-loading (dynamic import — not in initial bundle)
- [ ] 2.2 Build key generation flow: name/email/passphrase form → OpenPGP.js keygen → Argon2id-derive key → encrypt private key → upload blob + public key
- [ ] 2.3 Build key import flow: paste/upload armored private key → re-encrypt with passphrase → upload blob
- [ ] 2.4 Build key unlock flow: download blob → passphrase prompt → decrypt in browser → cache in `sessionStorage`
- [ ] 2.5 Build key management page: list keys (fingerprint, uid, created), set primary, export public/private, delete (with "messages will be unrecoverable" warning)
- [ ] 2.6 Implement auto-detect on message open: check MIME `Content-Type` for `multipart/encrypted` or `multipart/signed` or inline PGP armor; lazy-load OpenPGP.js then process
- [ ] 2.7 Implement decrypt flow: unlock key if needed → `openpgp.decrypt()` → display plaintext
- [ ] 2.8 Implement verify flow: fetch sender public key from `contact_keys` (or trigger discovery) → `openpgp.verify()` → show verified/unverified/invalid badge
- [ ] 2.9 Add Sign and Encrypt toggles to compose toolbar; persist per-account default preference
- [ ] 2.10 In compose: on recipient add/change, call `GET /keys/discover?email=` for each recipient; show lock icon if key found, warning icon if not
- [ ] 2.11 Implement sign on send: `openpgp.sign()` → wrap as `multipart/signed` PGP/MIME
- [ ] 2.12 Implement encrypt on send: `openpgp.encrypt()` to all recipient keys + own key → wrap as `multipart/encrypted` PGP/MIME; block send if any recipient key missing
- [ ] 2.13 Pre-enable Sign + Encrypt when replying to an encrypted message
- [ ] 2.14 Implement inline PGP detection and decryption (legacy `-----BEGIN PGP MESSAGE-----` in body)
