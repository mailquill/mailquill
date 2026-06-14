-- Two-way CalDAV sync metadata.
--
-- Events gain a stable iCalendar UID plus the server resource href and ETag so
-- sync can match local and remote copies, detect remote changes, and push local
-- edits. `dirty` marks a locally-changed event awaiting push; `deleted` is a
-- tombstone so a local delete is propagated to the server before the row is
-- removed. Calendars store their CalDAV collection URL.

ALTER TABLE calendar_events ADD COLUMN uid TEXT;
ALTER TABLE calendar_events ADD COLUMN href TEXT;
ALTER TABLE calendar_events ADD COLUMN etag TEXT;
ALTER TABLE calendar_events ADD COLUMN dirty INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calendar_events ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calendar_events ADD COLUMN updated_at TEXT;

ALTER TABLE calendars ADD COLUMN dav_url TEXT;

-- One row per (calendar, iCal UID). Partial so the many pre-sync rows with a
-- NULL uid don't collide.
CREATE UNIQUE INDEX IF NOT EXISTS idx_events_calendar_uid
    ON calendar_events(calendar_id, uid) WHERE uid IS NOT NULL;
