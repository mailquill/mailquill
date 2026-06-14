-- Per-account sync strategy: 'idle' = IMAP push via IDLE (instant, holds a
-- connection open), 'interval' = periodic poll every sync_interval_secs.
-- Default 'idle' preserves the push behaviour for existing IMAP accounts; the
-- API providers (Gmail/Outlook) ignore it and always poll.
ALTER TABLE email_accounts ADD COLUMN sync_mode TEXT NOT NULL DEFAULT 'idle';

-- Per-folder opt-out. Only folders with sync_enabled = 1 are synced; all
-- folders are still discovered and listed so they can be toggled in settings.
ALTER TABLE folders ADD COLUMN sync_enabled INTEGER NOT NULL DEFAULT 1;
