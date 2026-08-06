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

## Quickstart

Two supported ways to run Mailquill in production: the prebuilt container
image via Docker Compose, or a native install managed by systemd.

### Docker Compose (prebuilt image)

Multi-arch images (`linux/amd64`, `linux/arm64`) are published to GHCR for
every GitHub release. No toolchain required on the host.

```bash
mkdir mailquill && cd mailquill

# Data directory — the container runs as UID 1000
mkdir data && sudo chown 1000:1000 data

# Generate the two required secrets into .env
docker run --rm ghcr.io/mailquill/mailquill:latest secrets > .env
```

Create `docker-compose.yml`:

```yaml
services:
  app:
    image: ghcr.io/mailquill/mailquill:latest
    env_file: .env
    ports:
      - "8080:8080"
    volumes:
      - ./data:/data
    healthcheck:
      test: ["CMD", "wget", "-qO-", "http://localhost:8080/api/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 10s
    restart: unless-stopped
```

```bash
docker compose up -d
curl http://localhost:8080/api/health   # → {"status":"ok"}
```

Open `http://localhost:8080` and register the first account. If the GHCR
package is not public, authenticate first with `docker login ghcr.io`.
When serving behind a reverse proxy, set `APP_BASE_URL` in `.env` to the
public URL (required for OAuth redirects).

### Direct install with systemd

Every GitHub release ships prebuilt static Linux binaries (`x86_64` and
`aarch64`). Download the tarball for your architecture and install the binary
and the shipped phishing brand list — no toolchain required:

```bash
curl -LO https://github.com/mailquill/mailquill/releases/latest/download/mailquill-linux-x86_64.tar.gz
tar -xzf mailquill-linux-x86_64.tar.gz mailquill brands.json

sudo install -Dm755 mailquill /usr/local/bin/mailquill
sudo install -Dm644 brands.json /usr/local/share/mailquill/brands.json
```

To compile the binary yourself, see [Build from source](#build-from-source).

Releases also ship native binaries for macOS (`mailquill-macos-aarch64.tar.gz`,
`mailquill-macos-x86_64.tar.gz`) and Windows (`mailquill-windows-x86_64.zip`)
for running Mailquill outside of Linux servers.

Create a system user and the configuration:

```bash
sudo useradd --system --home-dir /var/lib/mailquill --no-create-home mailquill

sudo mkdir -p /etc/mailquill
mailquill secrets | sudo tee /etc/mailquill/mailquill.env >/dev/null
sudo tee -a /etc/mailquill/mailquill.env >/dev/null <<'EOF'
DATA_DIR=/var/lib/mailquill
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
# Public URL when behind a reverse proxy (required for OAuth redirects):
# APP_BASE_URL=https://mail.example.com
MAILQUILL_BRANDS_FILE=/usr/local/share/mailquill/brands.json
EOF
sudo chmod 600 /etc/mailquill/mailquill.env
```

Create `/etc/systemd/system/mailquill.service`:

```ini
[Unit]
Description=Mailquill web mail server
Wants=network-online.target
After=network-online.target

[Service]
User=mailquill
Group=mailquill
EnvironmentFile=/etc/mailquill/mailquill.env
ExecStart=/usr/local/bin/mailquill serve
StateDirectory=mailquill
Restart=on-failure
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

`StateDirectory=mailquill` lets systemd create `/var/lib/mailquill` with the
correct ownership; with `ProtectSystem=strict` it is the only writable path.

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now mailquill
curl http://localhost:8080/api/health   # → {"status":"ok"}
```

## Build from source

Requires Rust stable and Bun. `cargo xtask build` builds the frontend and the
release binary with the UI embedded via rust-embed:

```bash
cargo xtask build

sudo install -Dm755 target/release/mailquill /usr/local/bin/mailquill
sudo install -Dm644 backend/phishing/brands.json /usr/local/share/mailquill/brands.json
```

Continue with the service setup from
[Direct install with systemd](#direct-install-with-systemd).

## Requirements

- Rust stable
- Bun
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
cd frontend && bun install

# frontend dev server with hot reload (proxies the API)
bun run dev

# backend in debug mode (rust-embed serves frontend/dist from disk)
cargo run -p api
```

## Administration CLI

Reset a local Mailquill account password by piping the new password over stdin.
The command applies the same Argon2 policy as registration and revokes all
refresh sessions for the account:

```bash
read -rsp 'New password: ' mailquill_password
printf '\n'
printf '%s' "$mailquill_password" \
  | cargo run -p api -- reset-password --email user@example.com --password-stdin
unset mailquill_password
```

For the Docker Compose deployment, run the same command inside the app
container and point it at the mounted data volume:

```bash
read -rsp 'New password: ' mailquill_password
printf '\n'
printf '%s' "$mailquill_password" \
  | docker compose -f deploy/docker-compose.yml exec -T app \
      mailquill reset-password --email user@example.com --password-stdin --data-dir /data
unset mailquill_password
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
Contact synchronization rollout, CardDAV/TLS behavior, states, and rollback are
documented in [docs/contact-sync.md](docs/contact-sync.md).

Google:

1. Create an OAuth client in Google Cloud Console.
2. Enable Gmail API and People API access for the same project as the OAuth client.
3. Add `{APP_BASE_URL}/api/auth/oauth/google/callback` as an authorized redirect URI.
4. Configure `GOOGLE_OAUTH_CLIENT_ID` and `GOOGLE_OAUTH_CLIENT_SECRET`.

Microsoft:

1. Create an app registration in Microsoft Entra.
2. Add delegated mail permissions and `Contacts.ReadWrite`.
3. Add `{APP_BASE_URL}/api/auth/oauth/microsoft/callback` as a web redirect URI.
4. Configure `MICROSOFT_OAUTH_CLIENT_ID` and `MICROSOFT_OAUTH_CLIENT_SECRET`.

The frontend starts OAuth through `/api/auth/oauth/:provider/start`; the backend performs the token exchange and stores provider refresh tokens encrypted.

## Verification

```bash
cargo check

cd frontend
bun run test
bun run lint
bun run build
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
