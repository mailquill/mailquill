# SQLite Databases

Mailquill stores structured data in SQLite. There are **two separate database
schemas**, kept in separate files so that one user's mail data is physically
isolated from every other user's.

For where these files live on disk, see [filesystem-layout.md](filesystem-layout.md).
The database files are not encrypted by the application — protect them at rest
with volume-level encryption (LUKS/dm-crypt, gocryptfs, or an encrypted cloud
volume). IMAP/SMTP credentials inside `mail.db` are encrypted with
`CREDENTIAL_ENCRYPTION_KEY`.

## Connection configuration (WAL)

All connections open with the same WAL configuration
([backend/db/src/migrations.rs](../backend/db/src/migrations.rs#L11)), which
produces the `-wal` and `-shm` sidecar files next to each `.db`:

```
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA cache_size=-65536;     # 64 MiB page cache
PRAGMA mmap_size=268435456;   # 256 MiB mmap
PRAGMA temp_store=MEMORY;
PRAGMA foreign_keys=ON;
```

## `app.db` — global database

One file at `<data_dir>/app.db`, opened at startup
([backend/api/src/main.rs](../backend/api/src/main.rs#L231)). Holds
cross-user, account-level state. Migrations:
[backend/db/migrations/app/](../backend/db/migrations/app/).

| Table | Purpose |
|-------|---------|
| `users` | account records (id, email, password hash) |
| `refresh_tokens` | issued JWT refresh tokens, with grace-window support |
| `push_subscriptions` | Web Push (VAPID) browser subscriptions |
| `user_settings` | per-user preferences |
| `image_sender_allowlist` | senders allowed to load remote images |

The `id` in `users` is a 16-byte random value rendered as 32 lowercase hex
characters. **That hex string is the `<user_id>` used as the `users/` directory
name** for per-user databases and blobs.

## `mail.db` — per-user database

One file per user at `<data_dir>/users/<user_id>/mail.db`. Pools are opened
on demand and cached in an LRU
([backend/db/src/pool.rs](../backend/db/src/pool.rs)): at most 32 user pools are
kept open at once, and a pool idle for more than 600 s is evicted and reopened
on next use. Migrations:
[backend/db/migrations/mail/](../backend/db/migrations/mail/).

| Table / object | Purpose |
|----------------|---------|
| `email_accounts` | IMAP/SMTP accounts (encrypted credentials live here) |
| `account_aliases` | send-from aliases per account |
| `folders` | synced IMAP folders and their sync settings |
| `messages` | message metadata (headers, flags, thread keys) |
| `message_bodies` | pointer (`blob_key`) + sizes for each body in the blob store |
| `attachments` | attachment metadata + blob pointers |
| `remote_message_ids` | provider/UID ↔ local id mapping for sync |
| `messages_fts`, `messages_vocab` | FTS5 full-text search index + vocabulary |
| `phishing_analysis` | cached phishing verdicts per message |
| `pgp_keys`, `contact_keys` | PGP key material |
| `contacts`, `contact_accounts`, `contact_groups`, `contact_group_members` | contacts (CardDAV) |
| `calendars`, `calendar_accounts`, `calendar_events`, `meeting_invitations` | calendar (CalDAV) |
| `inbox_rules` | server-side / Sieve-style rules |
| `user_brand_entries` | user-defined brand entries for phishing checks |

Message *content* is **not** stored in these tables — only metadata and a blob
key. The bytes live in the [blob store](blob-store.md).
