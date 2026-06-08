## Context

Builds on the `foundation` change (workspace, blob store, PII redaction, DB schema, deployment all complete). This change implements all user-facing email functionality end-to-end: backend APIs and the full React frontend. Privacy is a first-class constraint — no external services, no telemetry, no CDN assets, no Gravatar.

## Decisions

### D1: App authentication — JWT + refresh token (httpOnly cookie)

Access token: 15-minute JWT in `Authorization` header. Refresh token: 30-day httpOnly cookie stored in `refresh_tokens` table as bcrypt hash (compare-only, never decrypted). Stateless access token enables horizontal scaling; httpOnly cookie prevents XSS token theft. App session tokens are separate from OAuth2 provider tokens (see D3).

### D2: Credential storage — AES-256-GCM, key in env

IMAP passwords, SMTP passwords, and OAuth2 refresh tokens for external providers stored encrypted in `email_accounts.credentials_encrypted BLOB`. Format: `[nonce(12B)][ciphertext][GCM tag(16B)]`. Key from `CREDENTIAL_ENCRYPTION_KEY` (64 hex chars); startup panics if absent or wrong length.

Operational rules enforced by code:
- `credentials_encrypted` never returned in any API response
- Credentials never written to logs at any level
- Decrypted credentials held in memory only during IMAP/SMTP connection, then dropped
- Integration test: `GET /accounts` response contains no credential fields

### D3: OAuth2 / XOAUTH2 — backend-only token exchange

Frontend redirects to provider; provider redirects back to backend callback; backend stores encrypted refresh token. Frontend never sees raw OAuth tokens. XOAUTH2 SASL string built in backend and injected into IMAP/SMTP auth. Providers: Gmail (`accounts.google.com`), Microsoft (`login.microsoftonline.com`). Library: `oauth2` crate.

### D4: IMAP sync — background task per account, polling

One tokio task per connected account. Default poll interval: 5 minutes (configurable via `sync_interval_secs` on `email_accounts`). No IMAP IDLE in v1 — polling only (complexity vs. benefit ratio for self-hosted). Manual "Refresh" button triggers immediate sync.

**Lazy sync (default):** `FETCH uid (FLAGS ENVELOPE INTERNALDATE BODY.PEEK[HEADER])` — headers + snippet stored, body skipped. Body fetched from IMAP on first message open, compressed (zstd), written to `BlobStore`, pointer stored in `message_bodies`.

**Full sync (opt-in per account):** `FETCH uid RFC822` — body fetched and stored during sync. Used when offline PWA access is needed.

**Incremental sync:** fetch UIDs > `folders.last_uid`. UIDVALIDITY change → purge folder rows + full re-sync.

### D5: Message storage — headers in DB, bodies in blob store

`messages` table: ~300 bytes/row (headers + metadata). Body never in DB. `message_bodies`: pointer table with `blob_key`, `size_bytes` (compressed), `size_bytes_uncompressed`. `snippet`: 160 chars pre-computed at sync time from ENVELOPE.

**On-demand body fetch:** `api` crate opens short-lived IMAP connection, fetches by UID, zstd-compresses, writes to `BlobStore`, inserts `message_bodies` row, returns decompressed bytes. Runs concurrently with sync task connection (IMAP servers allow multiple simultaneous connections). Mark message read only after body successfully loaded.

### D6: Message threading — IMAP THREAD with JWZ fallback

1. **IMAP THREAD**: if server CAPABILITY includes `THREAD=REFERENCES`, issue `UID THREAD REFERENCES UTF-8 ALL`; root message ID → `thread_id = hex(sha256(root_message_id)[..8])`.
2. **JWZ fallback**: walk References header chain to find root ancestor; assign same hash.
3. **Mailing list fallback**: if `List-Id` header present and no References chain links to existing thread, `thread_id = hex(sha256(list_id || ':' || subject_normalized)[..8])`.

Subject normalization: strip `Re:`, `Fwd:`, `AW:`, `FWD:`, `SV:`, `Sv:`, `Vs:` prefixes (case-insensitive, repeated). Stored in `subject_normalized`.

Thread sort: `internal_date ASC` (IMAP server-assigned, not forgeable).

### D7: SMTP send — `lettre` with XOAUTH2

`lettre` is the de-facto Rust SMTP library. Custom XOAUTH2 auth mechanism for Gmail/Outlook. `from` field validated: must be `primary_email` or a row in `account_aliases` owned by the authenticated user. Sent message APPENDed to IMAP Sent folder. Attachment size limit: 25 MB total.

### D8: Frontend state — TanStack Query + Zustand

TanStack Query: server state (mail data), caching, background refetch, optimistic updates. Zustand: UI-only state (selected account, compose open/closed, sidebar state). All TanStack Query hooks call `transport.*()` from the abstraction layer — never `fetch()` directly.

### D9: Privacy — no external assets, no tracking

Content-Security-Policy: `default-src 'self'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self' data: blob:; connect-src 'self'; frame-src 'none'; object-src 'none'`. Blocks all external loading at browser level. All fonts bundled locally in `frontend/public/fonts/`. No Google Fonts, CDN links, or tracking pixels. Contact avatar fallback: initial avatar (first letter of display name + deterministic colour from name hash) — no Gravatar (which reveals email addresses via MD5 to Automattic).

HTML message body rendered in sandboxed `<iframe>` to isolate external resource loads from remote email content.

### D10: Search — SQLite FTS5 / Postgres tsvector

SQLite mode: FTS5 virtual table `messages_fts(subject, from_addr, body_text)` with `contentless_delete=1`. Body text extracted from plain-text MIME part at body-write time and inserted into FTS. Postgres mode: `tsvector` + GIN index. No external search service — all embedded and self-hostable.
