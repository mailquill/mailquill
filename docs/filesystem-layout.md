# Local Data Layout

This is the overview of where Mailquill stores data on the local filesystem at
runtime. Each subsystem has its own document — see [Detailed docs](#detailed-docs)
below.

## The data directory

Everything persistent lives under a single **data directory**. It is configured
by the `data_dir` setting (env var `DATA_DIR`) and defaults to `./data`
relative to the process working directory. See
[backend/api/src/config.rs](../backend/api/src/config.rs#L43).

```
data/
├── app.db                 # global SQLite database (users, sessions, settings)
├── app.db-wal             # WAL journal (transient)
├── app.db-shm             # WAL shared-memory index (transient)
├── users/                 # one subdirectory per user account
│   └── <user_id>/
│       ├── mail.db        # per-user mail database
│       ├── mail.db-wal
│       └── mail.db-shm
├── blobs/                 # local blob store (when BLOB_BACKEND=local)
│   └── mail/<account_id>/<YYYY>/<MM>/<DD>/<uid>/...
├── brands.json            # brand list for phishing checks (operator override)
└── openphish.txt          # cached OpenPhish feed
```

The directory is created lazily — subdirectories appear the first time their
contents are written.

## Detailed docs

| Topic | Document |
|-------|----------|
| SQLite schemas (`app.db`, `mail.db`), WAL config, connection pooling | [databases.md](databases.md) |
| Blob store for message bodies & attachments (local / S3), key scheme, blob encryption | [blob-store.md](blob-store.md) |
| Phishing detection data files (`brands.json`, `openphish.txt`) | [phishing-data.md](phishing-data.md) |

## What is safe to delete

- `*.db-wal` / `*.db-shm` — transient; regenerated. Only delete while the server
  is stopped.
- `openphish.txt` — re-downloaded on next refresh.
- `brands.json` — re-seeded from the shipped default if removed.
- `app.db`, `users/<id>/mail.db`, `blobs/` — **persistent user data**; deleting
  these destroys accounts, mail, and cached message content. Back these up.
