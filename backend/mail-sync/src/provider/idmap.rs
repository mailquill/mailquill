//! Mapping between provider-native string message ids (Gmail / Graph) and the
//! per-folder integer uids the sync pipeline works with. Backed by the
//! `remote_message_ids` table; uids are assigned monotonically per folder so
//! the pipeline's `last_uid` incremental-sync logic keeps working.

use super::ProviderError;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use std::collections::HashSet;

const ASSIGN_BATCH_SIZE: usize = 500;

pub struct IdMap {
    db: SqlitePool,
    account_id: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct BackfillState {
    pub page_token: Option<String>,
    pub complete: bool,
}

impl IdMap {
    pub fn new(db: SqlitePool, account_id: String) -> Self {
        Self { db, account_id }
    }

    pub async fn known_ids(&self, folder: &str) -> Result<HashSet<String>, ProviderError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT remote_id FROM remote_message_ids WHERE account_id = ? AND folder_path = ?",
        )
        .bind(&self.account_id)
        .bind(folder)
        .fetch_all(&self.db)
        .await
        .map_err(wrap)?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    pub async fn backfill_state(&self, folder: &str) -> Result<BackfillState, ProviderError> {
        let state: Option<(Option<String>, bool)> = sqlx::query_as(
            "SELECT remote_backfill_page_token, remote_backfill_complete FROM folders WHERE account_id = ? AND full_path = ?",
        )
        .bind(&self.account_id)
        .bind(folder)
        .fetch_optional(&self.db)
        .await
        .map_err(wrap)?;
        let (page_token, complete) = state.ok_or_else(|| {
            ProviderError::Other(format!("folder not found for backfill state: {folder}"))
        })?;
        Ok(BackfillState {
            page_token,
            complete,
        })
    }

    pub async fn set_backfill_state(
        &self,
        folder: &str,
        page_token: Option<&str>,
        complete: bool,
    ) -> Result<(), ProviderError> {
        sqlx::query(
            "UPDATE folders SET remote_backfill_page_token = ?, remote_backfill_complete = ? WHERE account_id = ? AND full_path = ?",
        )
        .bind(page_token)
        .bind(complete)
        .bind(&self.account_id)
        .bind(folder)
        .execute(&self.db)
        .await
        .map_err(wrap)?;
        Ok(())
    }

    pub async fn max_uid(&self, folder: &str) -> Result<u32, ProviderError> {
        let max: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(uid) FROM remote_message_ids WHERE account_id = ? AND folder_path = ?",
        )
        .bind(&self.account_id)
        .bind(folder)
        .fetch_one(&self.db)
        .await
        .map_err(wrap)?;
        Ok(max.unwrap_or(0).clamp(0, u32::MAX as i64) as u32)
    }

    /// Assign ascending uids to `remote_ids` (must be ordered oldest first).
    /// Returns the new max uid.
    pub async fn assign(&self, folder: &str, remote_ids: &[String]) -> Result<u32, ProviderError> {
        let mut uid = self.max_uid(folder).await?;
        let mut seen = HashSet::new();
        let unique_ids: Vec<&String> = remote_ids
            .iter()
            .filter(|remote_id| seen.insert(remote_id.as_str()))
            .collect();

        for chunk in unique_ids.chunks(ASSIGN_BATCH_SIZE) {
            let first_uid = uid
                .checked_add(1)
                .ok_or_else(|| ProviderError::Other("remote id uid space exhausted".into()))?;
            let last_uid = uid
                .checked_add(chunk.len() as u32)
                .ok_or_else(|| ProviderError::Other("remote id uid space exhausted".into()))?;
            let mut query = QueryBuilder::<Sqlite>::new(
                "INSERT OR IGNORE INTO remote_message_ids (account_id, folder_path, uid, remote_id) ",
            );
            query.push_values(chunk.iter().enumerate(), |mut row, (offset, remote_id)| {
                row.push_bind(&self.account_id)
                    .push_bind(folder)
                    .push_bind(i64::from(first_uid) + offset as i64)
                    .push_bind(*remote_id);
            });
            query.build().execute(&self.db).await.map_err(wrap)?;
            uid = last_uid;
        }

        // `INSERT OR IGNORE` may skip an id that another sync already mapped;
        // report the durable maximum rather than the attempted sequence end.
        self.max_uid(folder).await
    }

    /// Store a specific (folder, uid) → remote_id mapping. Unlike [`assign`],
    /// the uid is given by the caller (the IMAP uid), not auto-incremented —
    /// used by the Gmail-over-IMAP hybrid to bind IMAP uids to Gmail message ids
    /// (hex X-GM-MSGID). Overwrites any prior remote_id for that uid.
    pub async fn set(&self, folder: &str, uid: u32, remote_id: &str) -> Result<(), ProviderError> {
        self.set_many(folder, &[(uid, remote_id.to_owned())])
            .await?;
        Ok(())
    }

    /// Store several IMAP uid → Gmail id mappings with one statement per
    /// bounded chunk. Gmail fetches these mappings in the same batches as mail
    /// headers, so committing every row separately only creates avoidable
    /// SQLite writer contention with the message import.
    pub async fn set_many(
        &self,
        folder: &str,
        mappings: &[(u32, String)],
    ) -> Result<(), ProviderError> {
        for chunk in mappings.chunks(ASSIGN_BATCH_SIZE) {
            let mut query = QueryBuilder::<Sqlite>::new(
                "INSERT INTO remote_message_ids (account_id, folder_path, uid, remote_id) ",
            );
            query.push_values(chunk, |mut row, (uid, remote_id)| {
                row.push_bind(&self.account_id)
                    .push_bind(folder)
                    .push_bind(i64::from(*uid))
                    .push_bind(remote_id);
            });
            query.push(
                " ON CONFLICT(account_id, folder_path, uid) DO UPDATE SET remote_id = excluded.remote_id",
            );
            query.build().execute(&self.db).await.map_err(wrap)?;
        }
        Ok(())
    }

    pub async fn remote_id(&self, folder: &str, uid: u32) -> Result<String, ProviderError> {
        let id: Option<String> = sqlx::query_scalar(
            "SELECT remote_id FROM remote_message_ids WHERE account_id = ? AND folder_path = ? AND uid = ?",
        )
        .bind(&self.account_id)
        .bind(folder)
        .bind(uid as i64)
        .fetch_optional(&self.db)
        .await
        .map_err(wrap)?;
        id.ok_or_else(|| ProviderError::Other(format!("no remote id for uid {uid} in {folder}")))
    }

    /// Resolve an IMAP-style uid set ("12", "5:20", "21:*", "1,4,9") to
    /// (uid, remote_id) pairs, ascending by uid.
    pub async fn resolve_set(
        &self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<(u32, String)>, ProviderError> {
        let mut out = Vec::new();
        for part in uid_set.split(',') {
            let part = part.trim();
            let rows: Vec<(i64, String)> = if let Some((start, end)) = part.split_once(':') {
                let start: i64 = start.parse().map_err(|_| bad_set(uid_set))?;
                if end == "*" {
                    sqlx::query_as("SELECT uid, remote_id FROM remote_message_ids WHERE account_id = ? AND folder_path = ? AND uid >= ? ORDER BY uid")
                        .bind(&self.account_id).bind(folder).bind(start)
                        .fetch_all(&self.db).await.map_err(wrap)?
                } else {
                    let end: i64 = end.parse().map_err(|_| bad_set(uid_set))?;
                    sqlx::query_as("SELECT uid, remote_id FROM remote_message_ids WHERE account_id = ? AND folder_path = ? AND uid BETWEEN ? AND ? ORDER BY uid")
                        .bind(&self.account_id).bind(folder).bind(start).bind(end)
                        .fetch_all(&self.db).await.map_err(wrap)?
                }
            } else {
                let uid: i64 = part.parse().map_err(|_| bad_set(uid_set))?;
                sqlx::query_as("SELECT uid, remote_id FROM remote_message_ids WHERE account_id = ? AND folder_path = ? AND uid = ?")
                    .bind(&self.account_id).bind(folder).bind(uid)
                    .fetch_all(&self.db).await.map_err(wrap)?
            };
            out.extend(rows.into_iter().map(|(uid, id)| (uid as u32, id)));
        }
        Ok(out)
    }

    /// Drop the mapping for a message that left the folder (move/delete);
    /// it gets re-discovered (with a fresh uid) wherever it shows up next.
    pub async fn remove(&self, folder: &str, uid: u32) -> Result<(), ProviderError> {
        sqlx::query(
            "DELETE FROM remote_message_ids WHERE account_id = ? AND folder_path = ? AND uid = ?",
        )
        .bind(&self.account_id)
        .bind(folder)
        .bind(uid as i64)
        .execute(&self.db)
        .await
        .map_err(wrap)?;
        Ok(())
    }
}

fn wrap(e: sqlx::Error) -> ProviderError {
    ProviderError::Other(format!("id map: {e}"))
}

fn bad_set(set: &str) -> ProviderError {
    ProviderError::Other(format!("invalid uid set: {set}"))
}

#[cfg(test)]
mod tests {
    use super::{BackfillState, IdMap};
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn backfill_cursor_round_trips_between_sync_runs() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE folders (
                account_id TEXT NOT NULL,
                full_path TEXT NOT NULL,
                remote_backfill_page_token TEXT,
                remote_backfill_complete INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO folders (account_id, full_path) VALUES ('account', 'INBOX')")
            .execute(&db)
            .await
            .unwrap();

        let ids = IdMap::new(db, "account".to_owned());
        assert_eq!(
            ids.backfill_state("INBOX").await.unwrap(),
            BackfillState::default()
        );

        ids.set_backfill_state("INBOX", Some("next/page+token"), false)
            .await
            .unwrap();
        assert_eq!(
            ids.backfill_state("INBOX").await.unwrap(),
            BackfillState {
                page_token: Some("next/page+token".to_owned()),
                complete: false,
            }
        );

        ids.set_backfill_state("INBOX", None, true).await.unwrap();
        assert_eq!(
            ids.backfill_state("INBOX").await.unwrap(),
            BackfillState {
                page_token: None,
                complete: true,
            }
        );
    }

    #[tokio::test]
    async fn assigns_large_remote_id_sets_in_order_without_duplicates() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE remote_message_ids (
                account_id TEXT NOT NULL,
                folder_path TEXT NOT NULL,
                uid INTEGER NOT NULL,
                remote_id TEXT NOT NULL,
                PRIMARY KEY (account_id, folder_path, uid),
                UNIQUE (account_id, folder_path, remote_id)
            )",
        )
        .execute(&db)
        .await
        .unwrap();

        let ids = IdMap::new(db.clone(), "account".to_owned());
        let mut remote_ids: Vec<String> =
            (0..1_200).map(|index| format!("id-{index:04}")).collect();
        remote_ids.push("id-0000".to_owned());

        assert_eq!(ids.assign("INBOX", &remote_ids).await.unwrap(), 1_200);

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM remote_message_ids")
            .fetch_one(&db)
            .await
            .unwrap();
        let first: String =
            sqlx::query_scalar("SELECT remote_id FROM remote_message_ids WHERE uid = 1")
                .fetch_one(&db)
                .await
                .unwrap();
        let last: String =
            sqlx::query_scalar("SELECT remote_id FROM remote_message_ids WHERE uid = 1200")
                .fetch_one(&db)
                .await
                .unwrap();

        assert_eq!(count, 1_200);
        assert_eq!(first, "id-0000");
        assert_eq!(last, "id-1199");
    }

    #[tokio::test]
    async fn stores_fetched_gmail_ids_in_batches_and_updates_conflicts() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE remote_message_ids (
                account_id TEXT NOT NULL,
                folder_path TEXT NOT NULL,
                uid INTEGER NOT NULL,
                remote_id TEXT NOT NULL,
                PRIMARY KEY (account_id, folder_path, uid),
                UNIQUE (account_id, folder_path, remote_id)
            )",
        )
        .execute(&db)
        .await
        .unwrap();

        let ids = IdMap::new(db.clone(), "account".to_owned());
        let mappings: Vec<(u32, String)> = (1..=1_200)
            .map(|uid| (uid, format!("gmail-{uid}")))
            .collect();
        ids.set_many("INBOX", &mappings).await.unwrap();
        ids.set("INBOX", 700, "gmail-700-updated").await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM remote_message_ids")
            .fetch_one(&db)
            .await
            .unwrap();
        let updated: String = sqlx::query_scalar(
            "SELECT remote_id FROM remote_message_ids WHERE account_id = 'account' AND folder_path = 'INBOX' AND uid = 700",
        )
        .fetch_one(&db)
        .await
        .unwrap();

        assert_eq!(count, 1_200);
        assert_eq!(updated, "gmail-700-updated");
    }
}
