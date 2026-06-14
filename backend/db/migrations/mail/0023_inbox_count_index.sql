-- The unified/folder list returns a total conversation count alongside the
-- page. That COUNT(*) filtered by folder (joined folder_type, or folder_id)
-- was a full scan of all non-deleted messages — ~800ms on a 200k-message
-- mailbox set, the dominant cost of the list endpoint. This partial index lets
-- the count run as a covering index scan per folder (~15ms).
CREATE INDEX IF NOT EXISTS idx_msg_folder_undeleted
    ON messages(folder_id) WHERE is_deleted = 0;
