## ADDED Requirements

### Requirement: Background sync per account
The system SHALL run a background sync task for each connected email account that polls IMAP on a configurable interval (default: 5 minutes).

#### Scenario: Sync starts on account creation
- **WHEN** a user adds a new email account
- **THEN** an initial full sync starts immediately and subsequent polls follow the configured interval

#### Scenario: Sync stops on account deletion
- **WHEN** a user deletes an email account
- **THEN** the sync task for that account is cancelled

---

### Requirement: Incremental sync using UIDs
The sync engine SHALL use IMAP UIDs to perform incremental syncs, fetching only messages with UIDs greater than the last seen UID per folder.

#### Scenario: Incremental fetch
- **WHEN** a sync runs after messages have already been fetched
- **THEN** only new messages (UID > last stored UID) are fetched and stored

#### Scenario: UID validity change (UIDVALIDITY)
- **WHEN** the server reports a changed UIDVALIDITY for a folder
- **THEN** the system purges all stored messages for that folder and performs a full re-sync

---

### Requirement: Folder discovery
The system SHALL list and sync all IMAP folders/mailboxes for each account, including standard folders (INBOX, Sent, Drafts, Trash, Spam) and any custom folders.

#### Scenario: Folder list fetched
- **WHEN** an account is synced
- **THEN** all subscribed folders are discovered and stored

---

### Requirement: Body sync mode — lazy default
Each account SHALL have a configurable `body_sync_mode` (`lazy` | `full`). Default is `lazy`.

In **lazy mode**: sync fetches headers only (`FETCH uid (FLAGS ENVELOPE BODY.PEEK[HEADER])`). A 160-character snippet SHALL be pre-computed from the ENVELOPE subject and first available header for message list rendering. Body is NOT stored until the message is opened.

In **full mode**: sync fetches headers and full body (`FETCH uid RFC822`). Body bytes are written to blob storage; a pointer record is created in `message_bodies`. Body content is never stored as a database column.

#### Scenario: Lazy sync stores headers only
- **WHEN** an account with `body_sync_mode=lazy` is synced
- **THEN** headers, flags, and snippet are stored in `messages`; no blob is written and no `message_bodies` row is created

#### Scenario: Full sync stores body in blob store
- **WHEN** an account with `body_sync_mode=full` is synced
- **THEN** headers are stored in `messages`; body bytes are written to blob store; a pointer row (blob_key, size_bytes) is inserted in `message_bodies`; extracted plain text is indexed in `messages_fts`

---

### Requirement: On-demand body fetch
When a message with no stored body is opened, the system SHALL fetch the body from IMAP by UID, write it to blob storage, record the blob key in `message_bodies`, and return the body to the client. Subsequent opens SHALL read from blob storage via the stored key.

#### Scenario: On-demand fetch writes to blob store
- **WHEN** a client requests a message whose body is not yet stored (`message_bodies` row absent)
- **THEN** the system opens a short-lived IMAP connection, fetches the body by UID, writes bytes to blob store, records blob_key in `message_bodies`, and streams the body to the client

#### Scenario: Body already stored
- **WHEN** a client requests a message whose body is already stored (`message_bodies` row present)
- **THEN** the system reads the blob from blob storage by blob_key and returns it; no IMAP connection opened

---

### Requirement: Message storage
The system SHALL store message headers (From, To, Cc, Subject, Date, Message-ID, In-Reply-To, References) and pre-computed snippet in the `messages` table. Body content SHALL be stored in blob storage only — never in a database column. The `message_bodies` table holds only a pointer (blob_key) to the stored body.

#### Scenario: New message stored
- **WHEN** a new message is fetched from IMAP
- **THEN** headers and snippet are stored in `messages`; body written to blob store and pointer stored in `message_bodies` only if `body_sync_mode=full`

---

### Requirement: Sync error handling
Transient IMAP errors (network timeout, temporary auth failure) SHALL NOT crash the sync task. The task SHALL log the error and retry on the next poll interval.

#### Scenario: Network timeout during sync
- **WHEN** an IMAP connection times out mid-sync
- **THEN** the sync task logs the error, marks the account sync status as "error", and retries at the next interval

---

### Requirement: Sync status visibility
The system SHALL expose a sync status per account (idle / syncing / error / last_synced_at) readable by the frontend.

#### Scenario: Status query
- **WHEN** the frontend requests `GET /accounts/:id/sync-status`
- **THEN** the system returns current sync status and last successful sync timestamp
