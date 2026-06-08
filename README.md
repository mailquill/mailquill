# Mailquill

Mailquill is a self-hosted web mail client with local user accounts, encrypted IMAP/SMTP credentials, background sync, message threading, search, account privacy controls, and a React frontend.

## Requirements

- Rust stable
- pnpm
- A 64-character hex `CREDENTIAL_ENCRYPTION_KEY`
- A `JWT_SECRET` value for signing application access tokens

## Run

Single command — builds the frontend and starts the release server with the UI
embedded into the binary via rust-embed:

```bash
./scripts/start.sh
```

Configuration (`CREDENTIAL_ENCRYPTION_KEY`, `JWT_SECRET`, …) is read from `.env`
in the repository root. The backend serves the embedded frontend and API on
`http://localhost:8080`.

`make run` is equivalent. To reuse an already-built `frontend/dist` and skip the
frontend build, run `SKIP_FRONTEND=1 ./scripts/start.sh`.

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
| `APP_BASE_URL` | no | Public backend base URL used for OAuth redirect URIs. Defaults to `http://localhost:8080`. |
| `GOOGLE_OAUTH_CLIENT_ID` | for Gmail OAuth | Google OAuth application client ID. |
| `GOOGLE_OAUTH_CLIENT_SECRET` | for Gmail OAuth | Google OAuth application client secret. |
| `MICROSOFT_OAUTH_CLIENT_ID` | for Outlook OAuth | Microsoft OAuth application client ID. |
| `MICROSOFT_OAUTH_CLIENT_SECRET` | for Outlook OAuth | Microsoft OAuth application client secret. |

## OAuth Apps

Google:

1. Create an OAuth client in Google Cloud Console.
2. Enable Gmail API access for the project.
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

These checks run against a started Mailquill server. Set `MAILQUILL_BASE_URL` when the server is not on `http://127.0.0.1:8080`.

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
MAILQUILL_APP_URL=http://127.0.0.1:8080 \
CHROME_PATH=/usr/bin/chromium \
node scripts/e2e/csp-console-check.mjs
```
