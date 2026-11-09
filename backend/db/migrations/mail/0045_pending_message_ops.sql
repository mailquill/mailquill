-- Durable outbox for moves and permanent deletes that still have to reach the
-- mail server.
--
-- Deleting, archiving or moving a message hides the local row at once and used
-- to queue the IMAP MOVE/EXPUNGE in an in-memory channel, one command per
-- message, each on its own connection. That command was lost whenever the sync
-- task was absent, the process restarted, or the server rejected or throttled
-- the connection - the message then stayed on the server while the client
-- showed it gone. Persisting the intended operation keeps it pending until the
-- server confirms it, lets one connection push a whole batch, and gives up
-- (restoring the local row) only after repeated failures.
CREATE TABLE IF NOT EXISTS pending_message_ops (
    id           INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    account_id   TEXT    NOT NULL REFERENCES email_accounts(id) ON DELETE CASCADE,
    folder_path  TEXT    NOT NULL,
    uid          INTEGER NOT NULL,
    -- 'move' (into dest_folder) or 'expunge' (permanent delete in folder_path).
    op           TEXT    NOT NULL,
    -- Logical destination ("Trash", "Archive") or an explicit folder path;
    -- NULL for 'expunge'.
    dest_folder  TEXT,
    attempts     INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    -- One pending operation per message: a later action on the same message
    -- replaces the earlier one instead of queueing a contradictory command.
    UNIQUE (account_id, folder_path, uid)
);

CREATE INDEX IF NOT EXISTS idx_pending_message_ops_account
    ON pending_message_ops (account_id, folder_path);
