### Requirement: Health endpoint
The system SHALL expose a health endpoint at `GET /api/health` that returns HTTP 200 with `{"status":"ok"}` when the server is running.

#### Scenario: Server running
- **WHEN** a client sends `GET /api/health`
- **THEN** server returns HTTP 200 with JSON body `{"status":"ok"}`

### Requirement: Frontend SPA serving
The system SHALL serve the compiled frontend SPA from the binary for all non-`/api/*` paths. Requests to unknown paths SHALL return `index.html` to support client-side routing.

#### Scenario: Root path returns index.html
- **WHEN** a client requests `GET /`
- **THEN** server returns HTTP 200 with `Content-Type: text/html` and `Cache-Control: no-cache`

#### Scenario: Static asset served with long cache
- **WHEN** a client requests a hashed static asset (e.g. `GET /assets/main.abc123.js`)
- **THEN** server returns HTTP 200 with correct `Content-Type` and `Cache-Control: public, max-age=31536000, immutable`

#### Scenario: Unknown path falls back to index.html
- **WHEN** a client requests `GET /inbox`
- **THEN** server returns HTTP 200 with `index.html` content

### Requirement: Blob storage backend selection
The system SHALL select a blob storage backend at startup based on the `BLOB_BACKEND` environment variable. Supported values are `local` (default) and `s3`.

#### Scenario: Local backend selected by default
- **WHEN** `BLOB_BACKEND` is unset
- **THEN** the system uses `LocalBlobStore` with path from `BLOB_LOCAL_PATH`

#### Scenario: S3 backend selected
- **WHEN** `BLOB_BACKEND=s3` and all required S3 vars are present
- **THEN** the system uses `S3BlobStore`

#### Scenario: S3 backend missing required vars
- **WHEN** `BLOB_BACKEND=s3` and any required S3 var is absent
- **THEN** the system panics at startup with a clear error message naming the missing variable

### Requirement: Blob encryption at rest
When enabled, the system SHALL encrypt all blob writes with AES-256-GCM using a per-blob random nonce. Callers SHALL be unaware of encryption.

#### Scenario: Encryption enabled with valid key
- **WHEN** `BLOB_ENCRYPTION=true` and `BLOB_ENCRYPTION_KEY` is a valid 64-hex-char string
- **THEN** all `put` calls encrypt data and all `get` calls decrypt transparently

#### Scenario: Encryption enabled without key
- **WHEN** `BLOB_ENCRYPTION=true` and `BLOB_ENCRYPTION_KEY` is absent
- **THEN** the system panics at startup with a clear error message

### Requirement: PII log redaction
The system SHALL redact personally identifiable information from log output according to `LOG_PII_MODE`. Default mode is `remove`.

#### Scenario: Remove mode redacts PII
- **WHEN** `LOG_PII_MODE=remove` (or unset) and a value wrapped in `Pii()` is logged
- **THEN** the logged output shows `[REDACTED]` instead of the raw value

#### Scenario: Hash mode produces stable output
- **WHEN** `LOG_PII_MODE=hash` and the same PII value is logged twice
- **THEN** both log entries show the same `pii:sha256:<16-hex-chars>` token

#### Scenario: Plaintext mode emits startup warning
- **WHEN** `LOG_PII_MODE=plaintext`
- **THEN** the server emits a warning to stderr at startup before serving any requests

### Requirement: Per-user SQLite database isolation
In SQLite mode the system SHALL open a separate `mail.db` file per user under `data/users/{user_id}/`. The directory SHALL be created if absent. Per-user migrations SHALL run on first open.

#### Scenario: First access creates directory and DB
- **WHEN** `UserDbPool.get(user_id)` is called for a user with no existing data directory
- **THEN** `data/users/{user_id}/` is created, `mail.db` is opened, and all mail migrations are applied

#### Scenario: Subsequent access reuses pool
- **WHEN** `UserDbPool.get(user_id)` is called for a user whose pool is already cached
- **THEN** the cached `SqlitePool` is returned without reopening the file

### Requirement: Single self-contained binary
The system SHALL compile to a single statically-linked binary that embeds the frontend dist and requires no external runtime dependencies.

#### Scenario: Binary starts without frontend/dist present at runtime
- **WHEN** the compiled binary is copied to a machine without the source tree
- **THEN** `GET /` returns the embedded `index.html` successfully
