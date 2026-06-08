## Why

With the project scaffold in place, `email-core` builds the complete, working email client: user registration and login, connecting IMAP/SMTP accounts (including OAuth2/XOAUTH2 for Gmail and Outlook), background message sync, sending mail, unified inbox, conversation threading, per-message actions, full-text search, and the entire corresponding frontend. This is the primary deliverable — a self-hosted, multi-account, multi-user email client with Gmail-like UX.

## What Changes

- User registration, login, and session management (JWT + httpOnly refresh token cookie)
- Email account CRUD: IMAP + SMTP config, auth scheme selection, credential encryption, per-account sync settings, account aliases
- OAuth2 / XOAUTH2 flow for Gmail and Outlook (backend-only token exchange)
- Background IMAP sync engine: folder discovery, header-only lazy sync (default), full body sync (opt-in), incremental UID-based updates, UIDVALIDITY change handling
- On-demand body fetch for lazy accounts: IMAP fetch on message open, body written to blob store
- MIME parsing: multipart bodies, attachment extraction, snippet generation
- FTS index population at sync time
- Conversation threading: IMAP THREAD command with JWZ fallback and mailing-list fallback
- Message actions: read/unread, flag, archive, delete, move
- SMTP send with reply/forward, multipart attachments, sent-folder APPEND
- Unified inbox and per-folder/per-account message list API (cursor-paginated)
- Full-text and filter-based search API
- Complete React frontend: app shell, auth pages, account management, mailbox UI, compose, search

## Capabilities

### New Capabilities

- `user-auth`: App-level user registration, login, session management (JWT/cookie)
- `email-account-management`: CRUD for connecting/configuring email accounts per user (IMAP + SMTP + auth scheme)
- `imap-sync`: Background IMAP sync engine — fetch, cache, and index messages from all connected accounts
- `smtp-send`: Compose and send email via per-account SMTP configuration
- `unified-mailbox`: Aggregated inbox view merging messages from all accounts; folder/label navigation
- `message-thread`: Conversation threading, read/unread state, flagging, deletion, archiving
- `search`: Full-text and header search across all synced messages

### Modified Capabilities

## Impact

- Depends on `foundation` change (workspace, DB, blob store, PII redaction all in place)
- Rust additions: `async-imap`, `lettre`, `argon2`, `jsonwebtoken`, `oauth2`, `mailparse`
- Frontend additions: all auth, mailbox, compose, search UI components
- New API routes: `/auth/*`, `/accounts/*`, `/auth/oauth/*`, `/mailbox/*`, `/messages/*`, `/threads/*`, `/send`, `/search`
- Content-Security-Policy header enforced on all responses: `default-src 'self'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self' data: blob:; connect-src 'self'; frame-src 'none'; object-src 'none'`
- All fonts used by shadcn/Tailwind bundled locally (no Google Fonts or CDN)
- Contact avatar fallback: locally generated initial avatar (first letter + deterministic colour from name hash) — no Gravatar
