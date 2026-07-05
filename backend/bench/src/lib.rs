//! Synthetic mailbox generator for the mail-list performance benchmark.
//!
//! Builds a `mail.db` with the production schema (via the `db` crate's
//! migrations) populated with several large mailboxes, so the list queries can
//! be benchmarked against realistic data volumes.

use rand::prelude::*;
use rand::rngs::StdRng;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::Path;

/// Open (creating if needed) a mail db pool with the production WAL pragmas and
/// migrations applied.
pub async fn open_pool(path: &Path) -> anyhow::Result<SqlitePool> {
    let opts = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await?;
    sqlx::query(db::migrations::WAL_PRAGMAS)
        .execute(&pool)
        .await?;
    db::migrations::run_mail_migrations(&pool)
        .await
        .map_err(|e| anyhow::anyhow!("migrations: {e}"))?;
    Ok(pool)
}

const FOLDER_TYPES: &[&str] = &["INBOX", "SENT", "ARCHIVE", "DRAFTS", "SPAM", "TRASH"];

/// Generate `accounts` mailboxes, each with a random number of messages in
/// `[min_msgs, max_msgs]`. Deletes any existing db at `path` first.
pub async fn generate(
    path: &Path,
    accounts: usize,
    min_msgs: usize,
    max_msgs: usize,
) -> anyhow::Result<usize> {
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }

    let pool = open_pool(path).await?;
    let mut rng = StdRng::seed_from_u64(42);

    let senders: Vec<String> = (0..80)
        .map(|i| format!("Sender {i} <user{i}@example{}.org>", i % 12))
        .collect();

    let mut total_messages = 0usize;

    for a in 0..accounts {
        let acct_id = format!("account-{a:04}");
        sqlx::query(
            "INSERT INTO email_accounts (id, display_name, primary_email, imap_host, imap_port, imap_auth_scheme, smtp_host, smtp_port, smtp_auth_scheme, credentials_encrypted) VALUES (?, ?, ?, 'imap.example.org', 993, 'plain', 'smtp.example.org', 465, 'plain', X'')",
        )
        .bind(&acct_id)
        .bind(format!("Mailbox {a}"))
        .bind(format!("user{a}@example.org"))
        .execute(&pool)
        .await?;

        let mut folders: Vec<(String, &str)> = Vec::new();
        for ft in FOLDER_TYPES {
            let fid = format!("{acct_id}-{ft}");
            sqlx::query(
                "INSERT INTO folders (id, account_id, name, full_path, folder_type) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&fid)
            .bind(&acct_id)
            .bind(*ft)
            .bind(*ft)
            .bind(*ft)
            .execute(&pool)
            .await?;
            folders.push((fid, ft));
        }
        let inbox_id = folders[0].0.clone();

        let n = rng.gen_range(min_msgs..=max_msgs);
        total_messages += n;

        let mut recent_threads: Vec<String> = Vec::new();
        let mut uid: HashMap<String, i64> = HashMap::new();

        let mut tx = pool.begin().await?;
        for i in 0..n {
            // Most mail lives in the inbox (the hot view); the rest is spread.
            let folder_id = if rng.gen_bool(0.75) {
                inbox_id.clone()
            } else {
                folders.choose(&mut rng).unwrap().0.clone()
            };
            let u = {
                let c = uid.entry(folder_id.clone()).or_insert(0);
                *c += 1;
                *c
            };

            // 30% of messages continue a recent thread; the rest start a new one.
            let thread_id = if !recent_threads.is_empty() && rng.gen_bool(0.30) {
                recent_threads.choose(&mut rng).unwrap().clone()
            } else {
                let t = format!("thread-{a}-{i}");
                recent_threads.push(t.clone());
                if recent_threads.len() > 400 {
                    recent_threads.remove(0);
                }
                t
            };

            // Sortable ISO-8601 dates spread across three years.
            let year = 2023 + rng.gen_range(0..3);
            let month = rng.gen_range(1..=12);
            let day = rng.gen_range(1..=28);
            let (hh, mm, ss) = (
                rng.gen_range(0..24),
                rng.gen_range(0..60),
                rng.gen_range(0..60),
            );
            let internal_date =
                format!("{year:04}-{month:02}-{day:02}T{hh:02}:{mm:02}:{ss:02}.000Z");
            let from = senders.choose(&mut rng).unwrap();

            sqlx::query(
                "INSERT INTO messages (account_id, folder_id, uid, thread_id, subject, subject_normalized, snippet, from_addr, internal_date, is_read, is_flagged) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&acct_id)
            .bind(&folder_id)
            .bind(u)
            .bind(&thread_id)
            .bind(format!("Subject line for message {i}"))
            .bind(format!("subject line for message {i}"))
            .bind("This is a representative snippet of the message body content.")
            .bind(from)
            .bind(&internal_date)
            .bind(rng.gen_bool(0.8) as i64)
            .bind(rng.gen_bool(0.05) as i64)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
    }

    Ok(total_messages)
}
