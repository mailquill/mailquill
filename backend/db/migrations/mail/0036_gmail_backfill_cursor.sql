-- Gmail discovery is intentionally bounded per sync run. Persist the opaque
-- next-page token so large labels resume after the bound instead of restarting
-- at the newest already-known message forever.
ALTER TABLE folders ADD COLUMN remote_backfill_page_token TEXT;
ALTER TABLE folders ADD COLUMN remote_backfill_complete INTEGER NOT NULL DEFAULT 0;

-- Older Gmail code removed a remote-id mapping for any metadata error without
-- marking the cached row deleted. Hide those orphan rows locally; if the
-- message still belongs to the label, discovery will assign it a new mapping
-- and fetch it again. This never changes the remote Gmail mailbox.
UPDATE messages
SET is_deleted = 1
WHERE is_deleted = 0
  AND account_id IN (
      SELECT id FROM email_accounts WHERE provider_kind = 'gmail_api'
  )
  AND NOT EXISTS (
      SELECT 1
      FROM folders f
      JOIN remote_message_ids r
        ON r.account_id = f.account_id
       AND r.folder_path = f.full_path
       AND r.uid = messages.uid
      WHERE f.id = messages.folder_id
  );

-- Remove mappings that no longer have a live cache row. A full backfill will
-- rediscover those remote ids and assign fresh uids; keeping a mapping to a
-- deleted row cannot repair it because deletion is intentionally sticky in
-- the message upsert path.
DELETE FROM remote_message_ids
WHERE account_id IN (
    SELECT id FROM email_accounts WHERE provider_kind = 'gmail_api'
)
AND NOT EXISTS (
    SELECT 1
    FROM folders f
    JOIN messages m
      ON m.folder_id = f.id
     AND m.uid = remote_message_ids.uid
     AND m.is_deleted = 0
    WHERE f.account_id = remote_message_ids.account_id
      AND f.full_path = remote_message_ids.folder_path
);

-- If cleanup lowered a label's maximum mapped uid, lower the fetch cursor as
-- well. Newly rediscovered ids are then guaranteed to fall beyond last_uid.
UPDATE folders
SET last_uid = MIN(
    COALESCE(last_uid, 0),
    COALESCE((
        SELECT MAX(r.uid)
        FROM remote_message_ids r
        WHERE r.account_id = folders.account_id
          AND r.folder_path = folders.full_path
    ), 0)
)
WHERE account_id IN (
    SELECT id FROM email_accounts WHERE provider_kind = 'gmail_api'
);
