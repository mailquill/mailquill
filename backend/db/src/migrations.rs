use sqlx::{migrate::MigrateError, SqlitePool};

pub async fn run_app_migrations(pool: &SqlitePool) -> Result<(), MigrateError> {
    sqlx::migrate!("./migrations/app").run(pool).await
}

pub async fn run_mail_migrations(pool: &SqlitePool) -> Result<(), MigrateError> {
    sqlx::migrate!("./migrations/mail").run(pool).await
}

pub const WAL_PRAGMAS: &str = "
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA cache_size=-65536;
PRAGMA mmap_size=268435456;
PRAGMA temp_store=MEMORY;
PRAGMA foreign_keys=ON;
";

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn memory_database() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql("PRAGMA foreign_keys=ON")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn fresh_mail_migration_is_repeatable() {
        let pool = memory_database().await;
        run_mail_migrations(&pool).await.unwrap();
        run_mail_migrations(&pool).await.unwrap();

        let table_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name IN ('contact_accounts', 'contact_books', 'contacts', 'contact_groups')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(table_count, 4);
    }

    #[tokio::test]
    async fn gmail_backfill_migration_repairs_orphans_and_stale_mappings() {
        let pool = memory_database().await;
        sqlx::raw_sql(
            "CREATE TABLE email_accounts (id TEXT PRIMARY KEY, provider_kind TEXT NOT NULL);
             CREATE TABLE folders (
                 id TEXT PRIMARY KEY,
                 account_id TEXT NOT NULL,
                 full_path TEXT NOT NULL,
                 last_uid INTEGER
             );
             CREATE TABLE messages (
                 id TEXT PRIMARY KEY,
                 account_id TEXT NOT NULL,
                 folder_id TEXT NOT NULL,
                 uid INTEGER NOT NULL,
                 is_deleted INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE remote_message_ids (
                 account_id TEXT NOT NULL,
                 folder_path TEXT NOT NULL,
                 uid INTEGER NOT NULL,
                 remote_id TEXT NOT NULL
             );
             INSERT INTO email_accounts VALUES ('gmail', 'gmail_api');
             INSERT INTO folders VALUES ('inbox', 'gmail', 'INBOX', 10);
             INSERT INTO messages VALUES ('mapped', 'gmail', 'inbox', 1, 0);
             INSERT INTO messages VALUES ('orphan', 'gmail', 'inbox', 2, 0);
             INSERT INTO messages VALUES ('deleted', 'gmail', 'inbox', 4, 1);
             INSERT INTO remote_message_ids VALUES ('gmail', 'INBOX', 1, 'remote-1');
             INSERT INTO remote_message_ids VALUES ('gmail', 'INBOX', 3, 'remote-3');
             INSERT INTO remote_message_ids VALUES ('gmail', 'INBOX', 4, 'remote-4');",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::raw_sql(include_str!(
            "../migrations/mail/0036_gmail_backfill_cursor.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();

        let orphan_deleted: bool =
            sqlx::query_scalar("SELECT is_deleted FROM messages WHERE id = 'orphan'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let (last_uid, page_token, complete): (i64, Option<String>, bool) = sqlx::query_as(
            "SELECT last_uid, remote_backfill_page_token, remote_backfill_complete FROM folders WHERE id = 'inbox'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(orphan_deleted);
        assert_eq!(last_uid, 1);
        assert_eq!(page_token, None);
        assert!(!complete);
        let remaining_mappings: Vec<String> =
            sqlx::query_scalar("SELECT remote_id FROM remote_message_ids ORDER BY remote_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(remaining_mappings, ["remote-1"]);
    }

    #[tokio::test]
    async fn gmail_imap_migration_switches_transport_and_clears_api_uid_cache() {
        let pool = memory_database().await;
        sqlx::raw_sql(
            "CREATE TABLE email_accounts (
                id TEXT PRIMARY KEY,
                provider_kind TEXT NOT NULL,
                sync_mode TEXT NOT NULL
             );
             CREATE TABLE folders (id TEXT PRIMARY KEY, account_id TEXT NOT NULL);
             CREATE TABLE messages (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                subject TEXT,
                from_addr TEXT
             );
             CREATE TABLE message_bodies (message_id TEXT NOT NULL);
             CREATE TABLE attachments (message_id TEXT NOT NULL);
             CREATE TABLE phishing_analysis (message_id TEXT NOT NULL);
             CREATE TABLE remote_message_ids (account_id TEXT NOT NULL);
             CREATE VIRTUAL TABLE messages_fts USING fts5(
                subject, from_addr, body_text, content=''
             );
             INSERT INTO email_accounts VALUES ('gmail', 'gmail_api', 'interval');
             INSERT INTO folders VALUES ('folder', 'gmail');
             INSERT INTO messages VALUES ('message', 'gmail', 'subject', 'sender@example.test');
             INSERT INTO message_bodies VALUES ('message');
             INSERT INTO attachments VALUES ('message');
             INSERT INTO phishing_analysis VALUES ('message');
             INSERT INTO remote_message_ids VALUES ('gmail');",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::raw_sql(include_str!(
            "../migrations/mail/0037_gmail_imap_idle_primary.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();

        let account: (String, String) = sqlx::query_as(
            "SELECT provider_kind, sync_mode FROM email_accounts WHERE id = 'gmail'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(account, ("gmail_imap".into(), "idle".into()));
        let messages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_one(&pool)
            .await
            .unwrap();
        let folders: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM folders")
            .fetch_one(&pool)
            .await
            .unwrap();
        let mappings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM remote_message_ids")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((messages, folders, mappings), (0, 0, 0));
    }

    #[tokio::test]
    async fn thread_hot_paths_use_covering_indexes_without_temporary_btrees() {
        let pool = memory_database().await;
        run_mail_migrations(&pool).await.unwrap();

        let thread_plan: Vec<String> = sqlx::query_as::<_, (i64, i64, i64, String)>(
            "EXPLAIN QUERY PLAN
             SELECT m.id, m.account_id, m.folder_id, m.uid, m.message_id_header,
                    m.in_reply_to, m.list_id, m.subject, m.from_addr, m.to_addrs,
                    m.snippet, m.internal_date, f.folder_type, f.full_path,
                    m.is_read, m.is_flagged
             FROM messages AS m INDEXED BY idx_msg_thread_undeleted_date
             LEFT JOIN folders AS f ON f.id = m.folder_id
             WHERE m.thread_id = ? AND m.is_deleted = 0
             ORDER BY m.internal_date ASC",
        )
        .bind("thread-id")
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|(_, _, _, detail)| detail)
        .collect();
        assert!(
            thread_plan
                .iter()
                .any(|detail| detail.contains("idx_msg_thread_undeleted_date")),
            "thread query did not use chronological partial index: {thread_plan:?}"
        );
        assert!(
            thread_plan
                .iter()
                .all(|detail| !detail.contains("USE TEMP B-TREE")),
            "thread query still needs a temporary sort: {thread_plan:?}"
        );

        let count_plan: Vec<String> = sqlx::query_as::<_, (i64, i64, i64, String)>(
            "EXPLAIN QUERY PLAN
             SELECT COUNT(DISTINCT COALESCE(message_id_header, id))
             FROM messages INDEXED BY idx_msg_thread_folder_identity_undeleted
             WHERE thread_id = ? AND folder_id = ? AND is_deleted = 0",
        )
        .bind("thread-id")
        .bind("folder-id")
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|(_, _, _, detail)| detail)
        .collect();
        assert!(
            count_plan
                .iter()
                .any(|detail| { detail.contains("idx_msg_thread_folder_identity_undeleted") }),
            "thread count did not use identity index: {count_plan:?}"
        );
        assert!(
            count_plan
                .iter()
                .all(|detail| !detail.contains("USE TEMP B-TREE")),
            "thread count still needs a temporary DISTINCT B-tree: {count_plan:?}"
        );
    }

    #[tokio::test]
    async fn mailbox_contact_migration_links_only_unambiguous_carddav_and_cascades() {
        let pool = memory_database().await;
        sqlx::raw_sql(
            "CREATE TABLE email_accounts (
                id TEXT PRIMARY KEY,
                carddav_url TEXT
             );
             CREATE TABLE contact_accounts (
                id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                type TEXT NOT NULL,
                base_url TEXT,
                auth_scheme TEXT NOT NULL DEFAULT 'basic'
             );
             CREATE TABLE contacts (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL REFERENCES contact_accounts(id) ON DELETE CASCADE,
                uid TEXT NOT NULL
             );
             CREATE TABLE contact_groups (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL REFERENCES contact_accounts(id) ON DELETE CASCADE,
                name TEXT NOT NULL
             );
             INSERT INTO email_accounts VALUES
                ('unique-mailbox', 'https://dav.example.test/addressbooks/user/'),
                ('ambiguous-a', 'https://shared.example.test/addressbooks'),
                ('ambiguous-b', 'https://shared.example.test/addressbooks/');
             INSERT INTO contact_accounts (id, display_name, type, base_url) VALUES
                ('unique-source', 'Unique', 'cardav', 'https://dav.example.test/addressbooks/user'),
                ('ambiguous-source', 'Ambiguous', 'cardav', 'https://shared.example.test/addressbooks'),
                ('other-source', 'Other', 'cardav', 'https://other.example.test/addressbooks');
             INSERT INTO contacts VALUES ('cached', 'unique-source', 'remote-1');",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::raw_sql(include_str!(
            "../migrations/mail/0034_mailbox_contact_integration.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();

        let linked: (Option<String>, String, i64) = sqlx::query_as(
            "SELECT email_account_id, management_mode,
                    (SELECT count(*) FROM contacts WHERE account_id = contact_accounts.id)
             FROM contact_accounts WHERE id = 'unique-source'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(linked, (Some("unique-mailbox".into()), "mailbox".into(), 1));

        let independent: Vec<(String, Option<String>, String)> = sqlx::query_as(
            "SELECT id, email_account_id, management_mode FROM contact_accounts
             WHERE id IN ('ambiguous-source', 'other-source') ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            independent,
            vec![
                ("ambiguous-source".into(), None, "independent".into()),
                ("other-source".into(), None, "independent".into()),
            ]
        );

        sqlx::query("DELETE FROM email_accounts WHERE id = 'unique-mailbox'")
            .execute(&pool)
            .await
            .unwrap();
        let cascaded: i64 =
            sqlx::query_scalar("SELECT count(*) FROM contact_accounts WHERE id = 'unique-source'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(cascaded, 0);
    }
}
