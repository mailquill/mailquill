## 1. User Authentication API

- [x] 1.1 Implement `POST /auth/register` — validate input, hash password with Argon2id, store user
- [x] 1.2 Implement `POST /auth/login` — verify password, issue JWT + httpOnly refresh token cookie
- [x] 1.3 Implement `POST /auth/refresh` — validate refresh token cookie, issue new JWT
- [x] 1.4 Implement `POST /auth/logout` — revoke refresh token, clear cookie
- [x] 1.5 Implement Axum JWT middleware extracting `user_id` for all protected routes

## 2. Email Account Management API

- [x] 2.1 Implement `POST /accounts` — accept IMAP/SMTP config + credentials, encrypt credentials, test IMAP connection, persist
- [x] 2.2 Implement `GET /accounts` — return all accounts for authenticated user (no credentials)
- [x] 2.3 Implement `PUT /accounts/:id` — update settings including `body_sync_mode`, re-encrypt, re-test connection
- [x] 2.4 Implement `DELETE /accounts/:id` — cascade delete messages and folders, cancel sync task
- [x] 2.5 Implement `GET /accounts/:id/sync-status` — return sync state + last_synced_at
- [x] 2.6 Implement `POST /accounts/:id/aliases` — add alias email + display name; validates email format
- [x] 2.7 Implement `GET /accounts/:id/aliases` — list all aliases for an account
- [x] 2.8 Implement `PATCH /accounts/:id/aliases/:alias_id` — update alias display name
- [x] 2.9 Implement `DELETE /accounts/:id/aliases/:alias_id` — remove alias
- [x] 2.10 Implement credential encryption/decryption: AES-256-GCM, random 12-byte nonce per row, format `[nonce(12B)][ciphertext][tag(16B)]` in `credentials_encrypted BLOB`; key from `CREDENTIAL_ENCRYPTION_KEY` env var (64 hex chars); panic at startup if absent or wrong length
- [x] 2.11 Enforce credential hygiene: never include `credentials_encrypted` in any API response; add integration test asserting `GET /accounts` response contains no credential fields; never log credentials at any level; decrypt in-memory only for duration of IMAP/SMTP connection
- [x] 2.12 Add user settings: `pgp_discovery_wkd_enabled` (default false), `pgp_discovery_keyserver_enabled` (default false); expose via `GET /settings` and `PATCH /settings`

## 3. OAuth2 / XOAUTH2

- [x] 3.1 Implement `GET /auth/oauth/:provider/start` — redirect to provider (Google, Microsoft) with PKCE
- [x] 3.2 Implement `GET /auth/oauth/:provider/callback` — exchange code for tokens, store encrypted refresh token
- [x] 3.3 Implement token refresh logic for XOAUTH2: auto-refresh access token before IMAP/SMTP use
- [x] 3.4 Build XOAUTH2 auth mechanism for `async-imap` (base64-encoded SASL string)
- [x] 3.5 Build XOAUTH2 auth mechanism for `lettre` SMTP

## 4. IMAP Sync Engine

- [x] 4.1 Implement folder discovery: `LIST "" "*"`, store in `folders` table
- [x] 4.2 Implement lazy sync (default): `FETCH uid (FLAGS ENVELOPE INTERNALDATE BODY.PEEK[HEADER])` — store headers + snippet in `messages`, skip `message_bodies`
- [x] 4.3 Implement full sync: `FETCH uid RFC822` — store headers in `messages`, compress body bytes with zstd (level 3), write compressed bytes to `BlobStore`, store blob_key + sizes in `message_bodies`
- [x] 4.4 Implement sync dispatch: check `body_sync_mode` per account, call lazy or full path accordingly
- [x] 4.5 Implement incremental sync: fetch UIDs > last_uid, handle UIDVALIDITY changes (purge folder + full re-sync)
- [x] 4.6 Parse message headers (From, To, Cc, Subject, Date, Message-ID, In-Reply-To, References, List-Id) and store IMAP `INTERNALDATE` as `internal_date`
- [x] 4.7 Pre-compute 160-char snippet from ENVELOPE subject + partial header at sync time
- [x] 4.8 Parse MIME body: extract text/plain and text/html parts, handle multipart; write each attachment to blob store; store attachment metadata in `attachments` table
- [x] 4.9 After body write: extract plain text, insert/update `messages_fts` FTS index
- [x] 4.10 Implement on-demand body fetch: `GET /messages/:id` checks `message_bodies` for blob_key; if missing, opens short-lived IMAP connection, fetches by UID, compresses, writes to blob store, inserts blob_key + sizes; on read: fetch blob, decompress, return bytes to client
- [x] 4.11 Mark message as read only after body is successfully loaded (not on header-only open)
- [x] 4.12 Implement sync task manager: spawn/cancel tokio tasks per account on create/delete
- [x] 4.13 Implement poll loop with configurable interval (default 5 min) and error handling
- [x] 4.14 Update `folders.unread_count` during sync based on `\Seen` flags
- [x] 4.15 Detect `text/calendar` MIME parts during message parse; store flag for meeting invitation handler (implemented in `calendar` change)

## 5. Message Threading

- [x] 5.1 During sync, check IMAP server CAPABILITY for `THREAD=REFERENCES`; if present issue `UID THREAD REFERENCES UTF-8 ALL` and parse server thread tree
- [x] 5.2 JWZ fallback: walk References header chain per message; find root ancestor; assign `thread_id = hex(sha256(root_message_id)[..8])`; re-check on arrival of messages that reference existing threads
- [x] 5.3 Mailing list fallback: parse `List-Id` header; if no References chain links message to existing thread, compute `thread_id = hex(sha256(list_id || ':' || subject_normalized)[..8])`
- [x] 5.4 Implement subject normalization function: strip `Re:`, `Fwd:`, `AW:`, `FWD:`, `SV:`, `Sv:`, `Vs:` prefixes (case-insensitive, repeated); store in `subject_normalized`
- [x] 5.5 Implement `GET /threads/:thread_id` — return all messages in thread sorted by `internal_date ASC`, cross-folder within user scope; include `folder_type` label per message
- [x] 5.6 Thread summary fields on message list response: `thread_size` (total count), `thread_unread` (unread count), `thread_participants` (up to 3 distinct from_addr values)
- [x] 5.7 Implement bulk thread actions: `POST /threads/:thread_id/archive`, `POST /threads/:thread_id/delete`, `PATCH /threads/:thread_id/read`

## 6. Message Actions API

- [x] 6.1 Implement `PATCH /messages/:id/read` — set is_read, queue IMAP `\Seen` flag update
- [x] 6.2 Implement `PATCH /messages/:id/flag` — toggle is_flagged, queue IMAP `\Flagged` update
- [x] 6.3 Implement `POST /messages/:id/archive` — IMAP MOVE to Archive folder
- [x] 6.4 Implement `DELETE /messages/:id` — IMAP MOVE to Trash (soft) or EXPUNGE (hard from Trash)
- [x] 6.5 Implement `POST /messages/:id/move` — IMAP MOVE to specified folder
- [x] 6.6 Implement async IMAP flag sync queue (process flag changes without blocking API response)

## 7. SMTP Send API

- [x] 7.1 Implement `POST /send` — compose + send via account's SMTP; `from` field must be either the account's `primary_email` or one of its `account_aliases`; reject if `from` address not owned by authenticated user
- [x] 7.2 Support reply/forward: inject In-Reply-To and References headers
- [x] 7.3 Support multipart attachments, enforce 25 MB total size limit
- [x] 7.4 Append sent message to IMAP Sent folder via APPEND command

## 8. Unified Mailbox & Folder API

- [x] 8.1 Implement `GET /mailbox/unified` — paginated merged INBOX across all accounts, cursor-based
- [x] 8.2 Implement `GET /accounts/:id/folders` — folder tree with unread counts
- [x] 8.3 Implement `GET /accounts/:id/folders/:folder/messages` — per-folder paginated message list
- [x] 8.4 Implement `GET /messages/:id` — full message detail including body and thread

## 9. Search API

- [x] 9.1 Implement `GET /search?q=...` — FTS query across all user's messages
- [x] 9.2 Support filter params: `from`, `to`, `subject`, `after`, `before`, `folder`, `account_id`, `is_read`, `is_flagged`
- [x] 9.3 Implement cursor-based pagination for search results (max 50 per page)

## 10. Frontend — App Shell & Routing

- [x] 10.1 Set up React Router routes: `/login`, `/register`, `/mail` (layout), `/mail/unified`, `/mail/:accountId/:folder`, `/mail/:accountId/:folder/:threadId`
- [x] 10.2 Implement auth guard (redirect to `/login` if no JWT)
- [x] 10.3 Implement JWT refresh interceptor in TanStack Query client
- [x] 10.4 Build app shell layout: left sidebar (accounts + folders), message list pane, message detail pane

## 11. Frontend — Auth Pages

- [x] 11.1 Build login page with email/password form and validation
- [x] 11.2 Build register page
- [x] 11.3 Wire OAuth2 "Connect Gmail" / "Connect Outlook" buttons in account setup

## 12. Frontend — Account Management UI

- [x] 12.1 Build "Add account" modal/page with IMAP/SMTP form fields, auth scheme selector, and `body_sync_mode` toggle (lazy = "Load on open" / full = "Download during sync")
- [x] 12.2 Build account list with sync status badge and last-synced timestamp
- [x] 12.3 Build account edit and delete actions
- [x] 12.4 Build privacy settings page: PGP key discovery toggles (WKD / keyserver, both off by default) with explanatory text noting that enabling them sends email addresses to external servers

## 13. Frontend — Mailbox UI

- [x] 13.1 Build sidebar with account list (unread badge) and folder tree per account
- [x] 13.2 Build unified inbox message list: thread rows showing participant names (up to 3), subject, latest snippet, date, per-thread unread badge, message count when > 1
- [x] 13.3 Build thread conversation view: stacked message cards sorted oldest→newest; most recent unread auto-expanded on open; click collapsed card to expand inline; show folder label per message (for cross-folder threads)
- [x] 13.4 Show mailing list badge (list name from List-Id) on thread rows and thread detail header when `list_id` is set
- [x] 13.5 Build message body renderer: formatted HTML body in sandboxed iframe; plain-text fallback; loading spinner during on-demand IMAP fetch
- [x] 13.6 Implement "Body not available offline" state: show message + "Download when online" option when offline and body not cached
- [x] 13.7 Implement read-on-open behavior (mark message read only after body successfully loaded)
- [x] 13.8 Implement thread-level toolbar actions: archive thread, delete thread, mark thread read/unread
- [x] 13.9 Implement per-message toolbar actions within thread: star, forward, reply, delete
- [x] 13.10 Implement "Refresh" button triggering manual sync via API

## 14. Frontend — Compose

- [x] 14.1 Build compose modal with To, Cc, Bcc, Subject, rich text body editor
- [x] 14.2 Implement reply and forward flows (pre-fill headers, quote original)
- [x] 14.3 Implement attachment upload with size validation
- [x] 14.4 Build From selector: dropdown groups entries as "Account (primary)" + indented aliases per account; populated from `GET /accounts` + `GET /accounts/:id/aliases`

## 15. Frontend — Search UI

- [x] 15.1 Build search bar in top nav with keyboard shortcut (Cmd/Ctrl+K)
- [x] 15.2 Build search results page with same message list component
- [x] 15.3 Implement filter chips (from, date range, account, read/unread)

## 16. Integration & Polish

- [x] 16.1 End-to-end test: register → add Gmail account (XOAUTH2) → sync → read message → reply
- [x] 16.2 End-to-end test: add plain IMAP account → sync → search → archive
- [x] 16.3 Verify multi-user isolation: two users cannot see each other's accounts or messages
- [x] 16.4 Verify `core/` crate has no compile-time dependency on `axum` or any HTTP type (CI check: `cargo tree -p core | grep axum` must return empty)
- [x] 16.5 Write README with setup instructions, env var reference, and OAuth app registration guide
- [x] 16.6 Document Tauri integration path in `tauri-app/README.md`: how to link `core/`, command mapping conventions, transport abstraction usage
- [x] 16.7 Audit all frontend dependencies: remove any that load remote resources at runtime; verify no Google Fonts, CDN links, or tracking pixels in any bundled code
- [x] 16.8 Set restrictive `Content-Security-Policy` response header in Axum: `default-src 'self'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self' data: blob:; connect-src 'self'; frame-src 'none'; object-src 'none'`
- [x] 16.9 Verify CSP blocks external resources: integration test that loading the app with CSP enabled produces no CSP violations in browser console
- [x] 16.10 Ensure all fonts used by shadcn/Tailwind are bundled locally (download and include in `frontend/public/fonts/`); configure Tailwind to reference local paths only
- [x] 16.11 Implement contact avatar fallback as locally generated initial avatar (first letter of display name + deterministic colour from name hash) — no Gravatar or external avatar service
