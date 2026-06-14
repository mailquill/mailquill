-- Unified inbox/folder-less listing orders all non-deleted messages by date
-- and joins folders for the folder_type filter. The existing indexes are all
-- folder_id/account_id prefixed, so that query fell back to a full scan plus a
-- temp B-tree sort (observed ~3s). This index serves "newest non-deleted
-- messages" directly: is_deleted equality + internal_date order, no filesort.
CREATE INDEX IF NOT EXISTS idx_msg_deleted_date
    ON messages(is_deleted, internal_date DESC);
