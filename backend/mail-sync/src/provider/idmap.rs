//! Mapping between provider-native string message ids (Gmail / Graph) and the
//! per-folder integer uids the sync pipeline works with. Backed by the
//! `remote_message_ids` table; uids are assigned monotonically per folder so
//! the pipeline's `last_uid` incremental-sync logic keeps working.

use super::ProviderError;
use sqlx::SqlitePool;
use std::collections::HashSet;

pub struct IdMap {
    db: SqlitePool,
    account_id: String,
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
        for remote_id in remote_ids {
            uid += 1;
            sqlx::query(
                "INSERT OR IGNORE INTO remote_message_ids (account_id, folder_path, uid, remote_id) VALUES (?, ?, ?, ?)",
            )
            .bind(&self.account_id)
            .bind(folder)
            .bind(uid as i64)
            .bind(remote_id)
            .execute(&self.db)
            .await
            .map_err(wrap)?;
        }
        Ok(uid)
    }

    /// Store a specific (folder, uid) → remote_id mapping. Unlike [`assign`],
    /// the uid is given by the caller (the IMAP uid), not auto-incremented —
    /// used by the Gmail-over-IMAP hybrid to bind IMAP uids to Gmail message ids
    /// (hex X-GM-MSGID). Overwrites any prior remote_id for that uid.
    pub async fn set(&self, folder: &str, uid: u32, remote_id: &str) -> Result<(), ProviderError> {
        sqlx::query(
            "INSERT INTO remote_message_ids (account_id, folder_path, uid, remote_id) VALUES (?, ?, ?, ?) ON CONFLICT(account_id, folder_path, uid) DO UPDATE SET remote_id = excluded.remote_id",
        )
        .bind(&self.account_id)
        .bind(folder)
        .bind(uid as i64)
        .bind(remote_id)
        .execute(&self.db)
        .await
        .map_err(wrap)?;
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
