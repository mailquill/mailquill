-- Durable outbox for flag changes that still have to reach the mail server.
--
-- Marking a message read updates the local row immediately and queues the IMAP
-- push in an in-memory channel. That push is lost whenever the sync task is
-- absent, the process restarts, or the IMAP command fails - and the next flag
-- reconciliation then copies the server's stale \Seen state back over the local
-- row, so the message pops up as unread again. Persisting the intended flag
-- keeps it pending until the server actually confirms it, and lets the
-- reconciliation skip fields with an outstanding push.
CREATE TABLE IF NOT EXISTS pending_flag_ops (
    id           INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    account_id   TEXT    NOT NULL REFERENCES email_accounts(id) ON DELETE CASCADE,
    folder_path  TEXT    NOT NULL,
    uid          INTEGER NOT NULL,
    flag         TEXT    NOT NULL,
    value        INTEGER NOT NULL,
    attempts     INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    -- One pending state per flag and message: a later change to the same flag
    -- replaces the earlier one instead of queueing a contradictory push.
    UNIQUE (account_id, folder_path, uid, flag)
);

CREATE INDEX IF NOT EXISTS idx_pending_flag_ops_account
    ON pending_flag_ops (account_id, folder_path);
