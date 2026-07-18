-- Sync progress is requested after every imported chunk. Counting all live
-- message-index entries each time becomes I/O-bound during a large concurrent
-- backfill, even with idx_msg_folder_undeleted. Keep the exact live count on
-- the folder row so progress reads only the account's small folder set.
ALTER TABLE folders
    ADD COLUMN live_message_count INTEGER NOT NULL DEFAULT 0;

UPDATE folders
SET live_message_count = (
    SELECT COUNT(*)
    FROM messages
    WHERE messages.folder_id = folders.id
      AND messages.is_deleted = 0
);

CREATE TRIGGER messages_live_count_after_insert
AFTER INSERT ON messages
WHEN NEW.is_deleted = 0
BEGIN
    UPDATE folders
    SET live_message_count = live_message_count + 1
    WHERE id = NEW.folder_id;
END;

CREATE TRIGGER messages_live_count_after_delete
AFTER DELETE ON messages
WHEN OLD.is_deleted = 0
BEGIN
    UPDATE folders
    SET live_message_count = MAX(live_message_count - 1, 0)
    WHERE id = OLD.folder_id;
END;

CREATE TRIGGER messages_live_count_after_visibility_change
AFTER UPDATE OF folder_id, is_deleted ON messages
WHEN OLD.folder_id <> NEW.folder_id OR OLD.is_deleted <> NEW.is_deleted
BEGIN
    UPDATE folders
    SET live_message_count = MAX(
        live_message_count - CASE WHEN OLD.is_deleted = 0 THEN 1 ELSE 0 END,
        0
    )
    WHERE id = OLD.folder_id;

    UPDATE folders
    SET live_message_count = live_message_count
        + CASE WHEN NEW.is_deleted = 0 THEN 1 ELSE 0 END
    WHERE id = NEW.folder_id;
END;
