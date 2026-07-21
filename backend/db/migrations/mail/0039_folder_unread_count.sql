-- Sidebar badges read folders.unread_count, while mailbox lists read live
-- messages directly. Repair any drift and keep the cached count exact for
-- every insert, delete, visibility change, read-state change, and folder move.
UPDATE folders
SET unread_count = (
    SELECT COUNT(*)
    FROM messages
    WHERE messages.folder_id = folders.id
      AND messages.is_read = 0
      AND messages.is_deleted = 0
);

CREATE TRIGGER messages_unread_count_after_insert
AFTER INSERT ON messages
WHEN NEW.is_read = 0 AND NEW.is_deleted = 0
BEGIN
    UPDATE folders
    SET unread_count = unread_count + 1
    WHERE id = NEW.folder_id;
END;

CREATE TRIGGER messages_unread_count_after_delete
AFTER DELETE ON messages
WHEN OLD.is_read = 0 AND OLD.is_deleted = 0
BEGIN
    UPDATE folders
    SET unread_count = MAX(unread_count - 1, 0)
    WHERE id = OLD.folder_id;
END;

CREATE TRIGGER messages_unread_count_after_state_change
AFTER UPDATE OF folder_id, is_read, is_deleted ON messages
WHEN OLD.folder_id <> NEW.folder_id
  OR OLD.is_read <> NEW.is_read
  OR OLD.is_deleted <> NEW.is_deleted
BEGIN
    UPDATE folders
    SET unread_count = MAX(
        unread_count
            - CASE WHEN OLD.is_read = 0 AND OLD.is_deleted = 0 THEN 1 ELSE 0 END,
        0
    )
    WHERE id = OLD.folder_id;

    UPDATE folders
    SET unread_count = unread_count
        + CASE WHEN NEW.is_read = 0 AND NEW.is_deleted = 0 THEN 1 ELSE 0 END
    WHERE id = NEW.folder_id;
END;
