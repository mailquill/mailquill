-- Performance indexes for messages table (task 4.17)
CREATE INDEX IF NOT EXISTS idx_msg_folder_date
    ON messages(folder_id, internal_date DESC);

CREATE INDEX IF NOT EXISTS idx_msg_foldertype_date
    ON messages(account_id, internal_date DESC);

CREATE INDEX IF NOT EXISTS idx_msg_account_folder_uid
    ON messages(account_id, folder_id, uid);

CREATE INDEX IF NOT EXISTS idx_msg_thread
    ON messages(thread_id);

CREATE INDEX IF NOT EXISTS idx_msg_message_id
    ON messages(message_id_header);

CREATE INDEX IF NOT EXISTS idx_msg_unread
    ON messages(folder_id, internal_date DESC)
    WHERE is_read = 0 AND is_deleted = 0;

CREATE INDEX IF NOT EXISTS idx_msg_flagged
    ON messages(folder_id, internal_date DESC)
    WHERE is_flagged = 1 AND is_deleted = 0;
