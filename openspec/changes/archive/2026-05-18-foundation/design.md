## Context

Brand new Rust + React project. No existing code to migrate. This change produces only infrastructure — no HTTP endpoints, no UI pages. Subsequent changes (`email-core`, `pwa`, etc.) layer features on top of what is built here.

## Decisions

### D1: Layered Rust workspace — `core/` library + thin adapter crates

All business logic lives in `core/` with zero dependency on Axum, HTTP types, or any network transport. Adapter crates (`api/`, `imap-sync/`, `calendar-sync/`, `smtp/`) consume `core/` and add transport concerns. This makes `core/` linkable by a future Tauri desktop client without refactoring.

```
backend/
├── core/        # Pure library: services, domain types, blob, PII, crypto utils
├── api/         # Axum handlers + rust-embed asset serving
├── db/          # SQLx models + migrations (app.db + per-user mail.db)
├── imap-sync/   # IMAP sync task runner
├── calendar-sync/
├── contact-sync/
└── smtp/
```

`core/` Cargo.toml: `tokio`, `sqlx`, `serde`, `uuid`, `tracing`, `argon2`, `async-imap`, `lettre`, `oauth2`, `icalendar`, `object_store`, `aes-gcm`, `zstd`, `sha2` — **no** `axum`.
`api/` Cargo.toml: `axum`, `rust-embed`, `serde_json`, `tower`, `jsonwebtoken` + depends on `core/`.

CI check: `cargo tree -p core | grep axum` must return empty.

### D2: Frontend transport abstraction

`frontend/src/transport/` exports a single `transport` object. Web builds use `http.ts` (fetch-based). Future Tauri builds use `tauri.ts` (invoke-based stub — not in initial bundle). All TanStack Query hooks call `transport.*()` — never `fetch()` directly. This is the only architectural constraint imposed on the frontend in this change; no pages are built here.

### D3: SQLite per-user DB split + Postgres mode

**SQLite (default):**
- `data/app.db` — one shared file: `users`, `refresh_tokens`, `push_subscriptions`, `user_settings`
- `data/users/{user_id}/mail.db` — one file per user: all mail data (accounts, folders, messages, keys, calendar, contacts). No `user_id` columns needed on mail tables — isolation is structural.

**Postgres mode:** single DB with `user_id` columns on all mail tables. Switched via `DATABASE_URL` env var format.

WAL pragmas set at every connection open for SQLite:
```sql
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA cache_size=-65536;
PRAGMA mmap_size=268435456;
PRAGMA temp_store=MEMORY;
PRAGMA foreign_keys=ON;
```

`UserDbPool` in `db/` crate: LRU cache of `SqlitePool` keyed by `user_id`, max 32 open pools, evict idle after 10 min. On first open for a user: create directory, open `mail.db`, run per-user migrations.

### D4: Blob storage — `BlobStore` trait + pluggable backends

Message bodies, attachments, and contact photos are never stored as DB columns. All binary content goes through a `BlobStore` trait in `core/src/blob.rs`:

```rust
#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, key: &str, data: Bytes) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Bytes>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn exists(&self, key: &str) -> Result<bool>;
}
```

Backends: `LocalBlobStore` (filesystem under `$BLOB_LOCAL_PATH`), `S3BlobStore` (via `object_store` crate — covers AWS S3, MinIO, Backblaze B2, Cloudflare R2 via `S3_ENDPOINT` override). Selected via `BLOB_BACKEND=local|s3`.

Optional `EncryptingBlobStore` wrapper: AES-256-GCM, random 12-byte nonce per blob, on-disk format `[nonce(12B)][ciphertext][GCM tag(16B)]`. Enabled via `BLOB_ENCRYPTION=true` + `BLOB_ENCRYPTION_KEY` (64 hex chars). Wraps any `Arc<dyn BlobStore>` transparently.

Body bytes are zstd-compressed (level 3) before being passed to `BlobStore::put`. Decompressed on read. Compression lives in `MessageService`, not inside the trait.

Key naming convention:
```
mail/{account_id}/{folder_id}/{yyyy}/{mm}/{dd}/{uid}/body
mail/{account_id}/{folder_id}/{yyyy}/{mm}/{dd}/{uid}/attach/{n}
contact/{account_id}/{uid}/photo
```
Date components from IMAP `INTERNALDATE` (server-assigned) — never the forgeable `Date` header.

### D5: PII log redaction — `Pii<T>` newtype

All `tracing` log fields containing Personal Identifiable Information are wrapped in `Pii<T>` defined in `core/src/pii.rs`. Behaviour controlled by `LOG_PII_MODE` env var:

| Mode | Output | Use case |
|---|---|---|
| `remove` (default) | `[REDACTED]` | Production |
| `hash` | `pii:sha256:<16-hex-chars>` | Ops correlation |
| `encrypt` | `pii:enc:<base64-aes-gcm>` | Auditable logs (requires `LOG_PII_KEY`) |
| `plaintext` | raw value | Development only |

Mode read once at startup into a global `AtomicU8`. Zero allocation overhead on hot path in `remove` mode. `plaintext` mode emits stderr warning at startup.

PII fields: `from_addr`, `to_addrs`, `cc_addrs`, `primary_email`, display names, subjects, IP addresses, calendar attendee emails, contact names.

### D6: Single binary — `rust-embed` + musl static linking

`api/` crate uses `rust-embed` to embed `frontend/dist/` into the binary at compile time. Axum catch-all route serves `index.html` for any non-`/api/*` path. Static assets served with correct `Content-Type` and cache headers.

Three musl targets:
- `x86_64-unknown-linux-musl` → `linux/amd64`
- `aarch64-unknown-linux-musl` → `linux/arm64`
- `armv7-unknown-linux-musleabihf` → `linux/arm/v7`

SQLx `bundled` feature statically compiles libsqlite3. Release profile: `strip = true`, `lto = true`, `codegen-units = 1`.

### D7: Container image — 3-stage Dockerfile, rootless uid 1000

Stage 1: `node:20-alpine` — builds `frontend/dist/`.
Stage 2: `rust:1-alpine` + `musl-dev` — compiles static binary for target arch (selected via `TARGETARCH`/`TARGETVARIANT` build args).
Stage 3: `alpine:3.20` + `ca-certificates` + `tzdata` — runtime only. User `mailquill` (uid 1000). `/data` volume. `EXPOSE 8080`. `ENTRYPOINT ["mailquill", "serve"]`.

Target final image size: ~30–35 MB.

### D8: Kubernetes security context (Helm default)

```yaml
securityContext:
  runAsNonRoot: true
  runAsUser: 1000
  readOnlyRootFilesystem: true
  allowPrivilegeEscalation: false
```

`readOnlyRootFilesystem: true` works because all writes go to the `/data` volume mount.
