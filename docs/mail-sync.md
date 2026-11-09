# Mail Sync

How mail reaches the local database, how local actions reach the mail
server, and how the web client learns about either. Code: `backend/mail-sync`
(sync task, providers), `backend/api/src/routes/messages.rs`, `threads.rs`,
`mailbox.rs` (local actions), `backend/api/src/sync_impl.rs` (events),
`frontend/src/shared/hooks/useMailNotifications.ts` and
`frontend/src/shared/lib/mailListCache.ts` (client side).

## One task per account

`SyncManager::start_account` spawns a Tokio task per mail account. The task
owns a bounded command channel (32 slots) and runs `run_sync_task_with_context`:

1. an initial full cycle,
2. a periodic ticker at the account's `sync_interval_secs` (missed ticks are
   not replayed; one poll after the next full interval is enough),
3. in `idle` sync mode an IMAP IDLE child on INBOX that requests a poll on
   server activity and reconnects with exponential backoff.

Each loop iteration waits for one wake-up (tick or command), then drains
everything already queued behind it. Queued provider commands run first, then
at most one sync cycle. Any number of poll requests (IDLE pokes, moves,
manual refreshes) collapse into that single cycle.

## A sync cycle

`do_sync` marks the account `syncing`, publishes the status, and runs
`sync_account` under a 30-minute ceiling (a hung connection must not block
the task forever; every chunk commits, so the next cycle resumes).

`sync_account`:

- pushes pending flag changes first (see below), so the reconciliation that
  follows cannot revert them;
- lists folders, upserts them into `folders` (new folders get the provider's
  default `sync_enabled`), and drops folders the server no longer lists
  unless they still hold local messages;
- runs a cheap status pre-pass over the enabled folders to get a stable
  progress denominator (`total`), publishes `synced / total`;
- syncs enabled folders in priority order (INBOX, Drafts, Sent, Archive,
  custom, Spam, Trash).

`sync_folder` for one folder:

- SELECTs the folder; a changed UIDVALIDITY purges and re-syncs it;
- reconciles flags of already stored messages (`reconcile_flags`);
- asks for the highest UID and backfills new UIDs in chunks (500 for IMAP,
  200 for the Gmail API), headers only in `lazy` body mode, full messages in
  `full` mode;
- persists each chunk in 100-row write transactions so API writes are never
  blocked for long, re-applies pending local flags on the rewritten rows,
  updates `folders.last_uid` and the unread counter, and publishes progress;
- sends new-message notifications for mail that arrived after the initial
  backfill and is not in Spam, Trash, Sent or Drafts.

Threading (`assign_thread_id`) attaches a message to an existing conversation
when its In-Reply-To or References name a stored Message-ID. Only the IDs a
chunk actually refers to are looked up (`referenced_threads`).

### Flag reconciliation

Header backfill only covers new UIDs, so read/flagged/deleted changes made
elsewhere have to be pulled explicitly:

- IMAP providers answer `search_flags` with four `UID SEARCH` queries (ALL,
  UNSEEN, FLAGGED, DELETED). Round trips stay constant in the folder size,
  and the ALL set is authoritative for which UIDs still exist, so UIDs that
  vanished are marked `is_deleted` locally (expunge detection).
- If the search fails or the provider cannot answer it (API providers), the
  chunked path fetches FLAGS for 1000 UIDs at a time.
- `apply_flag_reconciliation` copies server flags onto stored rows, never
  un-deletes a locally deleted row (`is_deleted = MAX(...)`), and keeps the
  local value of any flag that has a pending push (read per chunk, after the
  server fetch).

### Pending flag ops

Marking a message read or flagged must survive a missing sync task, a
process restart, or a failed IMAP command, and must not be reverted by the
next reconciliation. The API therefore writes a row to `pending_flag_ops`
*before* flipping the local flag, then nudges the task with `FlushFlags`.
Rows are deleted once the server accepts the flag; a flag the server keeps
rejecting is dropped after 10 attempts.

## Local actions

Delete, archive, move and not-spam update the local row first (`is_deleted =
1`) and only then queue the provider command (`IMapMove`, `IMapExpunge`).
Queueing never blocks the HTTP handler: a full channel hands the send to a
background task. The list therefore stays consistent immediately, and the
server-side move lands when the task processes its queue. Messages already in
Trash are expunged instead of moved. Bulk actions (`/mailbox/bulk`) apply one
UPDATE and queue the provider commands from a background task.

## Status and events

`SyncManager` keeps an in-memory `SyncStatus` per account (`state`,
`last_synced_at`, `error`, `synced`, `total`). It is pushed to open clients
over SSE (`/api/events`, event `sync`) when a cycle starts or ends and after
every persisted chunk; new mail is announced as event `message`. The
`GET /accounts/{id}/sync-status` endpoint derives `synced`/`total` from the
database so a reconnecting client gets consistent numbers.

States: `idle`, `syncing`, `error` (message in `error`), `reauth_required`
(OAuth token rejected; the task skips cycles until the account is
reconnected), `pending` (registered at startup, task not yet started).

## Client side

The web client holds one SSE connection (`useMailNotifications`) and feeds
it into TanStack Query:

- `sync` events update `['sync-status', accountId]` directly. A state change
  schedules a list refresh; progress-only events are throttled to one refresh
  per 3 s, because every refresh refetches each loaded page of the open
  infinite list.
- `message` and successful `send` events schedule a refresh too. The
  scheduler (`mailRefreshScheduler`) debounces for 500 ms but never waits
  longer than 2 s under a steady stream of events.
- After a reconnect the lists and sync states are refetched once; the stream
  has no replay.

List actions are optimistic (`mailListCache.ts`): rows and sidebar badges
change immediately, a failed request restores the snapshot and shows a toast,
and `onSettled` refetches the lists. Badges only move for changes the backend
confirms synchronously (unread counts are kept exact by database triggers on
`messages`); a destination count such as Trash rises with the next sync.

## On-demand bodies

In `lazy` body mode only headers are stored. Opening a message fetches its
body from the provider (`fetch_body_on_demand`), stores it in the blob store,
indexes it for search and runs the phishing analysis. The list prefetches the
newest message on load and a hovered row after 150 ms of hover intent.
`BodyFetchLimiter` caps concurrent downloads at two per account and keeps
successful sessions for 60 s so consecutive bodies reuse one connection.
