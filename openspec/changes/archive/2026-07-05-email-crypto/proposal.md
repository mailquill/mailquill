## Why

Email is inherently insecure in transit. Users who need end-to-end confidentiality or the ability to verify sender identity require OpenPGP support. Mailquill implements PGP/MIME + inline PGP with all cryptographic operations running in the browser — private keys never reach the server in plaintext. Key management uses a hybrid model: the server stores an opaque encrypted blob; the passphrase that unlocks it never leaves the client.

## What Changes

- Backend: PGP key blob storage API, public key discovery proxy (WKD / HKP, opt-in only), contact key cache
- Frontend: key generation, import, unlock, and management UI; auto-detection of encrypted/signed messages; decrypt and verify flows; sign and encrypt in compose; inline PGP support

## Capabilities

### New Capabilities

- `email-crypto`: OpenPGP sign/verify/encrypt/decrypt (client-side), key management, keyserver-based public key discovery

### Modified Capabilities

## Impact

- Depends on `email-core` (accounts, compose, message view, user settings)
- Backend additions: `pgp_keys` and `contact_keys` tables (already in `foundation` DB schema); new API routes for key management and discovery
- Frontend addition: `openpgp` npm package (lazy-loaded — ~600 KB gzipped, not in initial bundle); key management page; sign/encrypt compose toggles
- Opt-in external contact: WKD (`GET https://<domain>/.well-known/openpgpkey/hu/<hash>`) contacts recipient's domain only; HKP (`keys.openpgp.org`) contacts central keyserver — both off by default, user must enable per-account in settings
- Private keys never transmitted to server in plaintext; server stores opaque AES-256-GCM encrypted blob only
