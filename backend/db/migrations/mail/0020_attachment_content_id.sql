-- MIME Content-ID for inline attachments (cid: references in HTML bodies).
ALTER TABLE attachments ADD COLUMN content_id TEXT;
