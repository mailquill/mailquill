-- The unified-view top-K merge (per folder_id, newest first, live only)
-- otherwise lets the planner fall back to idx_msg_deleted_date — a global
-- is_deleted+date index that, for a folder where most rows are dead (e.g. a
-- Trash folder that's 98% permanently-deleted history never purged), has to
-- walk every other live message in the account before reaching this
-- folder's few survivors. This index serves "live messages in this folder,
-- newest first" directly, with no cross-folder rows to skip.
CREATE INDEX IF NOT EXISTS idx_msg_folder_undeleted_date
    ON messages(folder_id, internal_date DESC)
    WHERE is_deleted = 0;
