-- Thread readers filter out deleted messages and return chronological rows.
-- Keep that lookup ordered without scanning every row in the thread or
-- building a temporary B-tree.
CREATE INDEX IF NOT EXISTS idx_msg_thread_undeleted_date
    ON messages(thread_id, internal_date ASC)
    WHERE is_deleted = 0;

-- Message detail counts distinct logical messages inside one folder. Index
-- the exact COALESCE expression used by the query so SQLite can filter and
-- deduplicate from the covering index instead of scanning the message table
-- and materialising a temporary DISTINCT B-tree.
CREATE INDEX IF NOT EXISTS idx_msg_thread_folder_identity_undeleted
    ON messages(thread_id, folder_id, COALESCE(message_id_header, id))
    WHERE is_deleted = 0;
