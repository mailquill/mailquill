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
