# Blob Store — Message Bodies and Attachments

Raw message bodies and attachments are stored as opaque blobs, separately from
the [SQLite databases](databases.md) that hold their metadata. The `mail.db`
tables `message_bodies` and `attachments` keep only a `blob_key` pointer; the
bytes live here.

The store is a `BlobStore`
([backend/core/src/blob.rs](../backend/core/src/blob.rs)). The backend is
selected by `BLOB_BACKEND` (default `local`):

- **`local`** — files on disk under `BLOB_LOCAL_PATH` (default `./data/blobs`).
- **`s3`** — any S3-compatible bucket (`S3_BUCKET`, `S3_REGION`, `S3_ENDPOINT`,
  AWS credentials).

## Key scheme

Blob keys are deterministic paths derived from the message's account, internal
date, and UID:

```
mail/<account_id>/<YYYY>/<MM>/<DD>/<uid>/body          # message body
mail/<account_id>/<YYYY>/<MM>/<DD>/<uid>/attach/<n>    # nth attachment
```

With the local backend the key maps directly onto the directory tree, e.g.
`data/blobs/mail/<account_id>/2000/01/15/42/body`. With the S3 backend it is the
object key.

## Encryption at rest

When `BLOB_ENCRYPTION=true`, blobs are wrapped in an `EncryptingBlobStore` and
stored as `[nonce(12 B)][ciphertext][AES-256-GCM tag(16 B)]`, keyed by the
64-hex-char `BLOB_ENCRYPTION_KEY` (generate with `mailquill secrets`).

This is independent of:

- `CREDENTIAL_ENCRYPTION_KEY` — encrypts IMAP/SMTP credentials in `mail.db`.

The database files themselves are not encrypted by the application; protect them
with volume-level encryption (LUKS/dm-crypt, gocryptfs, or an encrypted cloud
volume). Encrypting the volume plus enabling blob encryption is what gives full
at-rest coverage of user data.
