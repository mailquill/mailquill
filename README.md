# Mailquill

Mailquill is your own private email app that you run on your own server. Bring
all your email accounts together in one inbox, search everything instantly, and
get a heads-up when a message looks like phishing — your mail stays with you,
not with a third party.

## Features

- **Multi-account unified inbox** — IMAP/SMTP accounts side by side, with
  conversation threading, a collapsible folder tree, and per-account views.
- **OAuth & plain IMAP** — connect Gmail through the Gmail API, Outlook via
  OAuth2/XOAUTH2, or any IMAP server manually, with automatic server
  autodiscovery.
- **Typo-tolerant search** — SQLite FTS5 full-text search with fuzzy matching,
  so "Decatlon" still finds "Decathlon".
- **Phishing detection** — SPF/DKIM/DMARC checks, display-name spoofing,
  look-alike/typosquatted sender domains, and OpenPhish feed matching, surfaced
  inline with localized explanations.
- **Push & live updates** — IMAP IDLE for near-instant delivery, Web Push
  notifications, and a server-sent-events stream to the UI.
- **Contacts & calendar** — CardDAV address books and CalDAV calendars.
- **Security by design** — local accounts with JWT sessions, account
  credentials encrypted at rest (AES-256-GCM), per-user SQLite databases.
- **Installable PWA** — responsive UI, dark mode, keyboard shortcuts, and
  English/German localization.

## Tech stack

- **Backend:** Rust (Axum, SQLx/SQLite, async-imap), workspace split into
  focused crates (api, mail-sync, phishing, smtp, contacts/calendar sync).
- **Frontend:** React + TypeScript, Vite, TanStack Query, React Router,
  i18next, served from the binary via rust-embed.

## Requirements

- Rust stable
- pnpm
- A 64-character hex `CREDENTIAL_ENCRYPTION_KEY`
- A `JWT_SECRET` value for signing application access tokens

## Run

Single command — builds the frontend and starts the release server with the UI
embedded into the binary via rust-embed. Cross-platform (Linux, macOS, Windows):

```bash
cargo dev
```

Configuration (`CREDENTIAL_ENCRYPTION_KEY`, `JWT_SECRET`, …) is read from `.env`
in the repository root. The backend serves the embedded frontend and API on
`http://localhost:8765` with the repo-local development `.env`.

`make run` and `./scripts/start.sh` are Unix-only equivalents. To reuse an
already-built `frontend/dist` and skip the frontend build, run
`SKIP_FRONTEND=1 cargo dev`. Extra flags are forwarded to the server binary via
`cargo xtask dev -- --some-flag`. Related tasks: `cargo xtask build`,
`cargo xtask clean`.

### Manual / development

```bash
# install frontend dependencies once
cd frontend && pnpm install

# frontend dev server with hot reload (proxies the API)
pnpm dev

# backend in debug mode (rust-embed serves frontend/dist from disk)
cargo run -p api
```

## Environment

| Variable | Required | Purpose |
| --- | --- | --- |
| `CREDENTIAL_ENCRYPTION_KEY` | yes | 64 hex chars used for AES-256-GCM credential encryption. |
| `JWT_SECRET` | yes | Secret used to sign 15-minute access tokens. |
| `DATA_DIR` | no | Storage directory for app and per-user databases. Defaults to `./data`. |
| `SERVER_PORT` | no | Backend bind port. The repo-local development `.env` uses `8765` to avoid common `8080` collisions. |
| `APP_BASE_URL` | no | Public backend base URL used for OAuth redirect URIs. The repo-local development `.env` uses `http://localhost:8765`. |
| `GOOGLE_OAUTH_CLIENT_ID` | for Gmail OAuth | Google OAuth application client ID. |
| `GOOGLE_OAUTH_CLIENT_SECRET` | for Gmail OAuth | Google OAuth application client secret. |
| `MICROSOFT_OAUTH_CLIENT_ID` | for Outlook OAuth | Microsoft OAuth application client ID. |
| `MICROSOFT_OAUTH_CLIENT_SECRET` | for Outlook OAuth | Microsoft OAuth application client secret. |

## OAuth Apps

For the full provider setup guide, including redirect URIs, scopes, and
troubleshooting, see [docs/oauth-gmail-outlook.md](docs/oauth-gmail-outlook.md).

Google:

1. Create an OAuth client in Google Cloud Console.
2. Enable Gmail API access for the same project as the OAuth client.
3. Add `{APP_BASE_URL}/api/auth/oauth/google/callback` as an authorized redirect URI.
4. Configure `GOOGLE_OAUTH_CLIENT_ID` and `GOOGLE_OAUTH_CLIENT_SECRET`.

Microsoft:

1. Create an app registration in Microsoft Entra.
2. Add delegated IMAP and SMTP permissions.
3. Add `{APP_BASE_URL}/api/auth/oauth/microsoft/callback` as a web redirect URI.
4. Configure `MICROSOFT_OAUTH_CLIENT_ID` and `MICROSOFT_OAUTH_CLIENT_SECRET`.

The frontend starts OAuth through `/api/auth/oauth/:provider/start`; the backend performs the token exchange and stores provider refresh tokens encrypted.

## Verification

```bash
cargo check

cd frontend
pnpm lint
pnpm build
```

## Live E2E Checks

These checks run against a started Mailquill server. Set `MAILQUILL_BASE_URL` when the server is not on `http://127.0.0.1:8765`.

Plain IMAP sync, search, and archive:

```bash
E2E_IMAP_EMAIL=user@example.com \
E2E_IMAP_HOST=imap.example.com \
E2E_IMAP_PORT=993 \
E2E_IMAP_USERNAME=user@example.com \
E2E_IMAP_PASSWORD=secret \
E2E_SMTP_HOST=smtp.example.com \
E2E_SMTP_PORT=587 \
E2E_SMTP_USERNAME=user@example.com \
E2E_SMTP_PASSWORD=secret \
node scripts/e2e/plain-imap-search-archive.mjs
```

Gmail XOAUTH2 sync, read, and reply after the Gmail account has been connected through the OAuth flow:

```bash
E2E_GMAIL_ACCOUNT_ID=<account-id> \
E2E_GMAIL_FROM=user@gmail.com \
node scripts/e2e/gmail-xoauth2-read-reply.mjs
```

CSP browser-console verification with a locally installed Chromium or Chrome:

```bash
MAILQUILL_APP_URL=http://127.0.0.1:8765 \
CHROME_PATH=/usr/bin/chromium \
node scripts/e2e/csp-console-check.mjs
```

## License

Copyright (C) 2026 Frank Gehann

Mailquill is free software: you can redistribute it and/or modify it under the
terms of the **GNU Affero General Public License** as published by the Free
Software Foundation, either version 3 of the License, or (at your option) any
later version (`AGPL-3.0-or-later`).

It is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR
PURPOSE. See the GNU Affero General Public License for more details.

Because Mailquill is typically run as a network service, the AGPL's §13 applies:
if you run a modified version and let users interact with it over a network, you
must offer those users the corresponding source of your modified version.

The full license text is in [LICENSE](LICENSE).
