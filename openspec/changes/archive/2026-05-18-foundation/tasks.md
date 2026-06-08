## 1. Project Scaffolding

- [x] 1.1 Initialize Rust workspace with `backend/` crates: `core`, `api`, `imap-sync`, `smtp`, `calendar-sync`, `contact-sync`, `db`
- [x] 1.2 `core/` Cargo.toml: deps `tokio`, `sqlx`, `serde`, `uuid`, `tracing`, `argon2`, `async-imap`, `lettre`, `oauth2`, `icalendar` — NO `axum` or HTTP transport deps
- [x] 1.3 `api/` Cargo.toml: deps `axum`, `rust-embed`, `serde_json`, `tower`, `jsonwebtoken`; depends on `core/`
- [x] 1.4 Define `core/` service structs: `AccountService`, `MessageService`, `CalendarService`, `ContactService`, `SyncEngine`, `SmtpService`, `CryptoService` — all accept `Arc<DbPool>` via constructor; bodies are `todo!()` stubs at this stage
- [x] 1.5 Initialize React app with Vite in `frontend/`
- [x] 1.6 Install frontend dependencies: `shadcn/ui`, `tailwindcss`, `@tanstack/react-query`, `react-router-dom`, `zustand`, `vite-plugin-pwa`
- [x] 1.7 Configure shadcn/ui and Tailwind CSS
- [x] 1.8 Create `frontend/src/transport/index.ts` — export `transport` object; `isTauri()` check selects `http.ts` vs `tauri.ts`; `tauri.ts` is a stub (no-op) in v1
- [x] 1.9 Wire all TanStack Query hooks through `transport.*()` — no bare `fetch()` calls in query functions (convention only at this stage; no hooks yet)
- [x] 1.10 Create `Makefile` with `build` target: `npm run build` in `frontend/` then `cargo build --release`
- [x] 1.11 Create `.env.example` with all required env vars (DB URL, JWT secret, OAuth client IDs, VAPID keys, `BLOB_BACKEND`, `BLOB_LOCAL_PATH`, `S3_BUCKET`, `S3_ENDPOINT`, `S3_REGION`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `BLOB_ENCRYPTION`, `BLOB_ENCRYPTION_KEY`, `CREDENTIAL_ENCRYPTION_KEY`, `LOG_PII_MODE`, `LOG_PII_KEY`) — include comment on each key: purpose, format, and whether required or optional

## 2. Blob Storage

- [x] 2.1 Add `object_store` crate to `core/` with `local` and `aws` features; define `BlobStore` trait (`put`, `get`, `delete`, `exists`)
- [x] 2.2 Implement `LocalBlobStore`: write/read files under `$BLOB_LOCAL_PATH/{key}`; create parent dirs on put
- [x] 2.3 Implement `S3BlobStore`: wrap `object_store` S3 backend; configure from `S3_BUCKET`, `S3_ENDPOINT`, `S3_REGION`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`; `S3_ENDPOINT` override enables MinIO / Backblaze B2 / Cloudflare R2
- [x] 2.4 Factory function: read `BLOB_BACKEND` env var, return `Arc<dyn BlobStore>` (local or S3); panic with clear message if required S3 vars absent when `BLOB_BACKEND=s3`
- [x] 2.5 Register `Arc<dyn BlobStore>` as Axum extension at startup
- [x] 2.6 Key naming helpers: `blob_key_body(account_id, folder_id, uid, internal_date: NaiveDate) -> String` → `mail/{account_id}/{folder_id}/{yyyy}/{mm}/{dd}/{uid}/body`; `blob_key_attachment(account_id, folder_id, uid, internal_date, n)` → `mail/{account_id}/{folder_id}/{yyyy}/{mm}/{dd}/{uid}/attach/{n}`; `folder_id` required because IMAP UIDs are folder-scoped
- [x] 2.7 On message/attachment delete: call `BlobStore::delete` for all blob keys belonging to that message; handle missing key gracefully (idempotent delete)
- [x] 2.8 Add `zstd` crate to `core/`; implement `compress_body(data: &[u8]) -> Vec<u8>` (level 3) and `decompress_body(data: &[u8]) -> Result<Vec<u8>>`; used by `MessageService` for all body read/write paths
- [x] 2.9 Add `aes-gcm` crate to `core/`; implement `EncryptingBlobStore` wrapper: on `put` encrypt bytes as `[nonce (12 B)][ciphertext][tag (16 B)]` with random nonce; on `get` split nonce, decrypt, verify tag; implements `BlobStore` trait, wraps any `Arc<dyn BlobStore>`
- [x] 2.10 Load `BLOB_ENCRYPTION_KEY` (64 hex chars) at startup; if `BLOB_ENCRYPTION=true` and key absent, panic with clear message; if `BLOB_ENCRYPTION=false` (default), skip wrapping
- [x] 2.11 Factory: when `BLOB_ENCRYPTION=true`, wrap the selected backend in `EncryptingBlobStore` before registering as Axum extension; callers receive `Arc<dyn BlobStore>` and are unaware of encryption
- [x] 2.12 Add `BLOB_ENCRYPTION` and `BLOB_ENCRYPTION_KEY` to `.env.example` with comment explaining key must be 64 hex chars (32 bytes)

## 3. PII Log Redaction

- [x] 3.1 Define `Pii<T>` newtype in `core/src/pii.rs`; implement `Display`, `Debug`, and `tracing::Value` — all delegate to the active mode
- [x] 3.2 Read `LOG_PII_MODE` at startup (`remove`|`hash`|`encrypt`|`plaintext`, default `remove`); store in global `AtomicU8`; panic if `encrypt` selected but `LOG_PII_KEY` absent or wrong length
- [x] 3.3 Implement each mode: `remove` → `"[REDACTED]"`; `hash` → `"pii:sha256:<16-hex-chars>"` (SHA-256 via `sha2` crate); `encrypt` → `"pii:enc:<base64-aes-gcm>"` (reuse `aes-gcm` from §2.9); `plaintext` → raw value + stderr warning at startup
- [x] 3.4 Wrap all PII fields at log call sites with `Pii(&value)`: email addresses, display names, message subjects, IP addresses, user-supplied strings in error messages, calendar attendee emails, contact names
- [x] 3.5 Add test: in `remove` mode `Pii("user@example.com")` formats as `[REDACTED]`; in `hash` mode output is stable (same input → same hash); in `plaintext` mode output is raw value
- [x] 3.6 CI lint rule (custom `clippy` or grep-based): reject any `tracing::info!/warn!/error!` that directly interpolates a known PII field name (`from_addr`, `to_addrs`, `primary_email`, `email`, `subject`) without `Pii()` wrapper

## 4. Database Schema & Migrations

**SQLite mode: two migration sets — `app.db` (shared) and `mail.db` (per-user)**

### 4a. App DB (`app.db`) — one file, shared across all users

- [x] 4.1 Create `users` table (id, email, password_hash, created_at)
- [x] 4.2 Create `refresh_tokens` table (id, user_id, token_hash, expires_at, revoked)
- [x] 4.3 Create `push_subscriptions` table (id, user_id, endpoint, p256dh, auth, created_at)
- [x] 4.4 Create `user_settings` table (user_id PK, body_sync_mode, pgp_discovery_wkd_enabled, pgp_discovery_keyserver_enabled, sign_by_default, always_encrypt)

### 4b. Per-user mail DB (`data/users/{user_id}/mail.db`) — one file per user, no `user_id` columns

- [x] 4.5 Create `email_accounts` table (id, display_name, primary_email, imap_host, imap_port, imap_auth_scheme, smtp_host, smtp_port, smtp_auth_scheme, credentials_encrypted, sync_interval_secs, body_sync_mode, created_at)
- [x] 4.6 Create `account_aliases` table (id, account_id FK, email TEXT NOT NULL, display_name TEXT, created_at)
- [x] 4.7 Create `folders` table (id, account_id, name, full_path, folder_type [INBOX/SENT/DRAFTS/TRASH/SPAM/ARCHIVE/CUSTOM], uidvalidity, last_uid, unread_count)
- [x] 4.8 Create `messages` table (id, account_id, folder_id, uid, message_id_header, thread_id, in_reply_to, references, list_id, subject, subject_normalized, snippet, from_addr, to_addrs, cc_addrs, date, internal_date, is_read, is_flagged, is_deleted, phishing_verdict, synced_at) — NO body columns; `internal_date` from IMAP `INTERNALDATE`; `snippet` 160 chars
- [x] 4.9 Create `message_bodies` table (message_id PK FK, blob_key TEXT NOT NULL, size_bytes INT, size_bytes_uncompressed INT, fetched_at)
- [x] 4.10 Create `attachments` table (id, message_id, filename, content_type, size_bytes, blob_key TEXT NOT NULL)
- [x] 4.11 Create `pgp_keys` table (id, fingerprint, uid, public_key_armored, private_key_encrypted_blob BLOB NOT NULL, is_primary, created_at)
- [x] 4.12 Create `contact_keys` table (id, email, public_key_data, source, fingerprint, fetched_at)
- [x] 4.13 Create calendar tables stub (id-only `calendar_accounts`, `calendars`, `calendar_events`, `meeting_invitations`) — full schema in `calendar` change
- [x] 4.14 Create contact tables stub (`contact_accounts`, `contacts`, `contact_groups`, `contact_group_members`) — full schema in `contacts` change
- [x] 4.15 Create `phishing_analysis` table (message_id PK FK, score INT, verdict TEXT, checks_json TEXT, analysed_at)
- [x] 4.16 Create `user_brand_entries` table (id, domain, brand_name)
- [x] 4.17 Create all performance indexes: `idx_msg_folder_date`, `idx_msg_foldertype_date`, `idx_msg_account_folder_uid`, `idx_msg_thread`, `idx_msg_message_id`, partial `idx_msg_unread`, partial `idx_msg_flagged`
- [x] 4.18 Create FTS5 virtual table: `messages_fts(subject, from_addr, body_text)` with `contentless_delete=1`; or `tsvector` + GIN index (Postgres)
- [x] 4.19 Apply SQLite WAL pragmas at connection open for both DB files

### 4c. Per-user DB connection manager

- [x] 4.20 Implement `UserDbPool` in `db/` crate: LRU cache of `SqlitePool` keyed by `user_id`, max 32 open pools, evict idle pools after 10 min
- [x] 4.21 On first open for a user: create `data/users/{user_id}/` directory, open `mail.db`, run per-user migrations
- [x] 4.22 Expose `UserDbPool` via Axum extension; middleware resolves the authenticated user's pool from the LRU cache on each request
- [x] 4.23 In Postgres mode: use single DB with `user_id` columns on all mail tables — `UserDbPool` returns the shared pool for all users

## 5. Single Binary — Embedded UI

- [x] 5.1 Add `rust-embed` to `api` crate; define `FrontendAssets` struct pointing to `../frontend/dist/`
- [x] 5.2 Add Axum catch-all route: serve embedded `index.html` for any path not matching `/api/*`
- [x] 5.3 Add Axum route for embedded static assets with correct `Content-Type` headers and cache headers
- [x] 5.4 Add `sqlx` feature `bundled` for SQLite to statically link libsqlite3 (required for musl builds)
- [x] 5.5 Set Cargo.toml release profile: `strip = true`, `lto = true`, `codegen-units = 1`
- [x] 5.6 Add `rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl armv7-unknown-linux-musleabihf` to dev setup docs
- [x] 5.7 Verify `cargo build --release --target x86_64-unknown-linux-musl` produces working binary

## 6. Container Image

- [x] 6.1 Write multi-stage `Dockerfile`: stage 1 `node:20-alpine` (frontend build), stage 2 `rust:1-alpine` + `musl-dev` (binary build), stage 3 `alpine:3.20` (runtime)
- [x] 6.2 Stage 3: `apk add --no-cache ca-certificates tzdata`, create user `adduser -D -u 1000 -g mailquill mailquill`, `mkdir /data && chown 1000:1000 /data`
- [x] 6.3 Stage 3: `COPY` binary, `USER 1000`, `VOLUME ["/data"]`, `EXPOSE 8080`, `ENTRYPOINT ["mailquill", "serve"]`
- [x] 6.4 Verify final image size is under 50 MB

## 7. Docker Compose

- [x] 7.1 Write `docker-compose.yml` (SQLite default): single `app` service, volume mount `./data:/data`, port `8080:8080`
- [x] 7.2 Write `docker-compose.postgres.yml` override: add `postgres:16` service, set `DATABASE_URL` env var on app service
- [x] 7.3 Add `healthcheck` to app service (`curl -f http://localhost:8080/api/health`)

## 8. Kubernetes & Helm

- [x] 8.1 Write Helm chart scaffold in `deploy/helm/mailquill/`: `Chart.yaml`, `values.yaml`, `templates/`
- [x] 8.2 `values.yaml`: `database.type` (sqlite/postgres), `persistence.size` (default 10Gi), `ingress.enabled`, `ingress.hostname`, `ingress.tls`, `replicaCount` (warn if >1 with sqlite), `image.tag`
- [x] 8.3 Write `templates/statefulset.yaml` (SQLite mode) with PVC and `/data` volume mount
- [x] 8.4 Write `templates/deployment.yaml` (Postgres mode, stateless)
- [x] 8.5 Write `templates/service.yaml` (ClusterIP, port 8080)
- [x] 8.6 Write `templates/ingress.yaml` with TLS support
- [x] 8.7 Write `templates/secret.yaml` for JWT secret and VAPID keys (with `existingSecret` override option)
- [x] 8.8 Set default `securityContext`: `runAsNonRoot: true`, `runAsUser: 1000`, `readOnlyRootFilesystem: true`, `allowPrivilegeEscalation: false`
- [x] 8.9 Write raw Kubernetes manifests in `deploy/k8s/` as alternative to Helm
- [x] 8.10 Write `README` section: Helm install command, required values, upgrade procedure

## 9. CI — Multi-arch Build & Publish

- [x] 9.1 Write GitHub Actions workflow: on tag push `vX.Y.Z`, build frontend, then build Rust binary for `linux/amd64`, `linux/arm64`, and `linux/arm/v7` using Docker buildx
- [x] 9.2 Configure buildx with QEMU for cross-compilation (`docker/setup-qemu-action`); QEMU supports all three platforms
- [x] 9.3 Use correct Rust target per platform in Dockerfile: `x86_64-unknown-linux-musl` (amd64), `aarch64-unknown-linux-musl` (arm64), `armv7-unknown-linux-musleabihf` (arm/v7) — use `TARGETARCH`/`TARGETVARIANT` build args to select
- [x] 9.4 Push multi-arch manifest to `ghcr.io/${{ github.repository }}:${{ github.ref_name }}` and `:latest`
- [x] 9.5 Verify arm/v7 image runs via `docker run --platform linux/arm/v7`
- [x] 9.6 Verify `core/` crate has no compile-time dependency on `axum` (CI check: `cargo tree -p core | grep axum` must return empty)
