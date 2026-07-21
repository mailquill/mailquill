//! Mail list queries shared by the API handlers and the performance benchmark.
//!
//! Kept here (rather than inline in the axum handlers) so the hot path is
//! testable and benchmarkable in isolation. The thread enrichment is batched:
//! one aggregate query for size/unread and one ordered fetch for participants,
//! instead of an N+1 over the page.

use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

/// Count live starred messages through the small partial starred index.
///
/// Without `INDEXED BY`, SQLite can choose `idx_msg_deleted_date` and scan
/// every live message even though `idx_msg_flagged` contains only the rows the
/// query needs. Optional unread/account predicates can safely be appended.
pub const STARRED_COUNT_SQL: &str =
    "SELECT COUNT(*) FROM messages INDEXED BY idx_msg_flagged WHERE is_flagged = 1 AND is_deleted = 0";

/// Sum trigger-maintained live counts for the folders enabled for sync.
///
/// This reads only the account's folder rows. Recounting the partial message
/// index after every imported chunk was normally quick, but became I/O-bound
/// while several account backfills were writing to the same SQLite database.
pub const SYNCED_MESSAGE_COUNT_SQL: &str =
    "SELECT COALESCE(SUM(live_message_count), 0) FROM folders WHERE account_id = ? AND sync_enabled = 1";

/// One conversation row as rendered in a mail list.
#[derive(Serialize, Clone)]
pub struct ThreadRow {
    pub message_id: String,
    pub thread_id: Option<String>,
    pub subject: String,
    pub from_addr: String,
    pub snippet: String,
    pub internal_date: String,
    pub is_read: bool,
    pub is_flagged: bool,
    pub account_id: String,
    pub folder_id: String,
    pub folder_type: String,
    pub folder_path: String,
    pub list_id: Option<String>,
    pub phishing_verdict: Option<String>,
    pub is_local_draft: bool,
    pub thread_size: i64,
    pub thread_unread: i64,
    pub thread_participants: Vec<String>,
}

/// A page of conversations plus pagination metadata.
pub struct Page {
    pub items: Vec<ThreadRow>,
    pub total: i64,
    pub next_cursor: Option<String>,
}

/// Raw projection from the list SELECT, before thread enrichment.
type RawRow = (
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    bool,
    bool,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    bool,
);

/// SQL predicate selecting the messages for a cross-account unified view.
pub fn view_filter(view: Option<&str>) -> &'static str {
    match view.unwrap_or("inbox") {
        "starred" => "m.is_flagged = 1",
        "sent" => "f.folder_type = 'SENT'",
        "drafts" => "f.folder_type = 'DRAFTS'",
        "archive" => "f.folder_type = 'ARCHIVE'",
        "spam" => "f.folder_type = 'SPAM'",
        "trash" => "f.folder_type = 'TRASH'",
        _ => "f.folder_type = 'INBOX'",
    }
}

const LIST_COLUMNS: &str = "m.id, m.thread_id, m.subject, m.from_addr, m.snippet, m.internal_date, m.is_read, m.is_flagged, m.account_id, m.folder_id, m.list_id, m.phishing_verdict, f.full_path, f.folder_type, m.is_local_draft";

/// Cross-account unified inbox/view page, newest first. Pass `account_id` to
/// scope the same view to a single mailbox (e.g. that account's starred list).
pub async fn unified_page(
    db: &SqlitePool,
    view: Option<&str>,
    account_id: Option<&str>,
    cursor: Option<&str>,
    limit: i64,
    unread: bool,
) -> Result<Page, sqlx::Error> {
    let filter = view_filter(view);
    let account_clause = if account_id.is_some() {
        "AND m.account_id = ?"
    } else {
        ""
    };
    let cursor_clause = if cursor.is_some() {
        "AND m.internal_date < ?"
    } else {
        ""
    };
    let unread_clause = if unread { "AND m.is_read = 0" } else { "" };
    let sql = format!(
        "SELECT {LIST_COLUMNS} FROM messages m JOIN folders f ON f.id = m.folder_id WHERE {filter} AND m.is_deleted = 0 {unread_clause} {account_clause} {cursor_clause} ORDER BY m.internal_date DESC LIMIT ?",
    );
    let mut q = sqlx::query_as::<_, RawRow>(&sql);
    if let Some(a) = account_id {
        q = q.bind(a);
    }
    if let Some(c) = cursor {
        q = q.bind(c);
    }
    let rows = q.bind(limit).fetch_all(db).await?;

    let next_cursor = rows.last().map(|r| r.5.clone());
    let has_more = rows.len() as i64 == limit;

    let total = view_total(db, view, account_id, unread).await;

    let items = enrich_threads(db, rows).await?;
    Ok(Page {
        items,
        total,
        next_cursor: if has_more { next_cursor } else { None },
    })
}

/// Total conversation count for a unified view.
///
/// A `COUNT(*)` joining `folders` for `folder_type` scans every matching
/// message and — without up-to-date ANALYZE statistics — the planner does not
/// use the partial index, costing ~800ms on a 200k mailbox set. Instead resolve
/// the view's folders and sum per-folder counts: a `folder_id = ?` equality is
/// served by the covering `idx_msg_folder_undeleted` deterministically, with no
/// dependence on planner statistics.
async fn view_total(
    db: &SqlitePool,
    view: Option<&str>,
    account_id: Option<&str>,
    unread: bool,
) -> i64 {
    let account_clause = if account_id.is_some() {
        "AND account_id = ?"
    } else {
        ""
    };
    let unread_clause = if unread { "AND is_read = 0" } else { "" };
    if view == Some("starred") {
        let sql = format!("{STARRED_COUNT_SQL} {unread_clause} {account_clause}");
        let mut q = sqlx::query_scalar::<_, i64>(&sql);
        if let Some(a) = account_id {
            q = q.bind(a);
        }
        return q.fetch_one(db).await.unwrap_or(0);
    }

    let folder_type = match view.unwrap_or("inbox") {
        "sent" => "SENT",
        "drafts" => "DRAFTS",
        "archive" => "ARCHIVE",
        "spam" => "SPAM",
        "trash" => "TRASH",
        _ => "INBOX",
    };

    let folders_sql = format!("SELECT id FROM folders WHERE folder_type = ? {account_clause}");
    let mut fq = sqlx::query_scalar::<_, String>(&folders_sql).bind(folder_type);
    if let Some(a) = account_id {
        fq = fq.bind(a);
    }
    let folder_ids: Vec<String> = fq.fetch_all(db).await.unwrap_or_default();

    let mut total = 0;
    let count_sql = format!(
        "SELECT COUNT(*) FROM messages WHERE folder_id = ? AND is_deleted = 0 {unread_clause}",
    );
    for fid in folder_ids {
        total += sqlx::query_scalar::<_, i64>(&count_sql)
            .bind(&fid)
            .fetch_one(db)
            .await
            .unwrap_or(0);
    }
    total
}

/// Single-folder page, newest first.
pub async fn folder_page(
    db: &SqlitePool,
    folder_id: &str,
    cursor: Option<&str>,
    limit: i64,
    unread: bool,
) -> Result<Page, sqlx::Error> {
    let cursor_clause = if cursor.is_some() {
        "AND m.internal_date < ?"
    } else {
        ""
    };
    let unread_clause = if unread { "AND m.is_read = 0" } else { "" };
    let sql = format!(
        "SELECT {LIST_COLUMNS} FROM messages m JOIN folders f ON f.id = m.folder_id WHERE m.folder_id = ? AND m.is_deleted = 0 {unread_clause} {cursor_clause} ORDER BY m.internal_date DESC LIMIT ?",
    );
    let mut q = sqlx::query_as::<_, RawRow>(&sql).bind(folder_id);
    if let Some(c) = cursor {
        q = q.bind(c);
    }
    let rows = q.bind(limit).fetch_all(db).await?;

    let next_cursor = rows.last().map(|r| r.5.clone());
    let has_more = rows.len() as i64 == limit;

    let total_unread_clause = if unread { "AND is_read = 0" } else { "" };
    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM messages WHERE folder_id = ? AND is_deleted = 0 {total_unread_clause}",
    ))
    .bind(folder_id)
    .fetch_one(db)
    .await
    .unwrap_or(0);

    let items = enrich_threads(db, rows).await?;
    Ok(Page {
        items,
        total,
        next_cursor: if has_more { next_cursor } else { None },
    })
}

/// Enrich the page's rows with thread size, unread count and the first few
/// participants — batched into two queries over all of the page's threads.
async fn enrich_threads(db: &SqlitePool, rows: Vec<RawRow>) -> Result<Vec<ThreadRow>, sqlx::Error> {
    let thread_ids: Vec<String> = {
        let mut seen = HashSet::new();
        rows.iter()
            .filter_map(|r| r.1.clone())
            .filter(|t| seen.insert(t.clone()))
            .collect()
    };

    let mut counts: HashMap<(String, String), (i64, i64)> = HashMap::new();
    let mut participants: HashMap<(String, String), Vec<String>> = HashMap::new();

    if !thread_ids.is_empty() {
        let placeholders = vec!["?"; thread_ids.len()].join(",");

        // `INDEXED BY idx_msg_thread` forces the thread index: without it, the
        // planner needs ANALYZE statistics to pick the index for a `thread_id
        // IN (...)` list and otherwise full-scans the table (~1s on a 200k
        // mailbox set). Pinning the index keeps enrichment deterministic (~1ms)
        // regardless of statistics freshness.
        // COUNT(DISTINCT …) so a message that lives in several folders (INBOX +
        // Archive) counts once toward the thread size; COALESCE keeps rows that
        // have no Message-ID distinct by their row id.
        let sql = format!(
            "SELECT thread_id, folder_id, COUNT(DISTINCT COALESCE(message_id_header, id)), SUM(CASE WHEN is_read = 0 THEN 1 ELSE 0 END) FROM messages INDEXED BY idx_msg_thread WHERE thread_id IN ({placeholders}) AND is_deleted = 0 GROUP BY thread_id, folder_id",
        );
        let mut q = sqlx::query_as::<_, (String, String, i64, i64)>(&sql);
        for id in &thread_ids {
            q = q.bind(id);
        }
        for (tid, folder_id, size, unread) in q.fetch_all(db).await? {
            counts.insert((tid, folder_id), (size, unread));
        }

        let sql_p = format!(
            "SELECT thread_id, folder_id, from_addr FROM messages INDEXED BY idx_msg_thread WHERE thread_id IN ({placeholders}) AND is_deleted = 0 ORDER BY internal_date ASC",
        );
        let mut qp = sqlx::query_as::<_, (String, String, String)>(&sql_p);
        for id in &thread_ids {
            qp = qp.bind(id);
        }
        for (tid, folder_id, addr) in qp.fetch_all(db).await? {
            let v = participants.entry((tid, folder_id)).or_default();
            if v.len() < 3 && !v.contains(&addr) {
                v.push(addr);
            }
        }
    }

    let items = rows
        .into_iter()
        .map(|r| {
            let (
                message_id,
                thread_id,
                subject,
                from_addr,
                snippet,
                internal_date,
                is_read,
                is_flagged,
                account_id,
                folder_id,
                list_id,
                phishing_verdict,
                folder_path,
                folder_type,
                is_local_draft,
            ) = r;

            let (thread_size, thread_unread, thread_participants) = match &thread_id {
                Some(tid) => {
                    let key = (tid.clone(), folder_id.clone());
                    let (size, unread) = counts
                        .get(&key)
                        .copied()
                        .unwrap_or((1, if is_read { 0 } else { 1 }));
                    let p = participants
                        .get(&key)
                        .cloned()
                        .unwrap_or_else(|| vec![from_addr.clone()]);
                    (size, unread, p)
                }
                None => (1, if is_read { 0 } else { 1 }, vec![from_addr.clone()]),
            };

            ThreadRow {
                message_id,
                thread_id,
                subject,
                from_addr,
                snippet,
                internal_date,
                is_read,
                is_flagged,
                account_id,
                folder_id,
                folder_type,
                folder_path,
                list_id,
                phishing_verdict,
                is_local_draft,
                thread_size,
                thread_unread,
                thread_participants,
            }
        })
        .collect();

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::{unified_page, STARRED_COUNT_SQL, SYNCED_MESSAGE_COUNT_SQL};
    use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

    async fn test_db() -> SqlitePool {
        let db = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE folders (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                full_path TEXT NOT NULL,
                folder_type TEXT NOT NULL,
                unread_count INTEGER NOT NULL DEFAULT 0,
                sync_enabled INTEGER NOT NULL DEFAULT 1,
                live_message_count INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE messages (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                folder_id TEXT NOT NULL,
                uid INTEGER NOT NULL,
                message_id_header TEXT,
                thread_id TEXT,
                subject TEXT NOT NULL DEFAULT '',
                snippet TEXT NOT NULL DEFAULT '',
                from_addr TEXT NOT NULL DEFAULT '',
                internal_date TEXT NOT NULL,
                is_read INTEGER NOT NULL DEFAULT 0,
                is_flagged INTEGER NOT NULL DEFAULT 0,
                is_deleted INTEGER NOT NULL DEFAULT 0,
                is_local_draft INTEGER NOT NULL DEFAULT 0,
                list_id TEXT,
                phishing_verdict TEXT
            )",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::raw_sql(
            "CREATE TRIGGER messages_live_count_after_insert
             AFTER INSERT ON messages WHEN NEW.is_deleted = 0 BEGIN
                 UPDATE folders SET live_message_count = live_message_count + 1
                 WHERE id = NEW.folder_id;
             END;
             CREATE TRIGGER messages_live_count_after_delete
             AFTER DELETE ON messages WHEN OLD.is_deleted = 0 BEGIN
                 UPDATE folders SET live_message_count = MAX(live_message_count - 1, 0)
                 WHERE id = OLD.folder_id;
             END;
             CREATE TRIGGER messages_live_count_after_visibility_change
             AFTER UPDATE OF folder_id, is_deleted ON messages
             WHEN OLD.folder_id <> NEW.folder_id OR OLD.is_deleted <> NEW.is_deleted BEGIN
                 UPDATE folders
                 SET live_message_count = MAX(
                     live_message_count - CASE WHEN OLD.is_deleted = 0 THEN 1 ELSE 0 END,
                     0
                 )
                 WHERE id = OLD.folder_id;
                 UPDATE folders
                 SET live_message_count = live_message_count
                     + CASE WHEN NEW.is_deleted = 0 THEN 1 ELSE 0 END
                 WHERE id = NEW.folder_id;
             END;",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("CREATE INDEX idx_msg_thread ON messages(thread_id)")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "CREATE INDEX idx_msg_flagged ON messages(folder_id, internal_date DESC) \
             WHERE is_flagged = 1 AND is_deleted = 0",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "CREATE INDEX idx_msg_folder_undeleted ON messages(folder_id) \
             WHERE is_deleted = 0",
        )
        .execute(&db)
        .await
        .unwrap();
        db
    }

    #[tokio::test]
    async fn starred_count_forces_the_partial_flagged_index() {
        let db = test_db().await;
        let plan: Vec<(i64, i64, i64, String)> =
            sqlx::query_as(&format!("EXPLAIN QUERY PLAN {STARRED_COUNT_SQL}"))
                .fetch_all(&db)
                .await
                .unwrap();

        assert!(
            plan.iter()
                .any(|(_, _, _, detail)| detail.contains("idx_msg_flagged")),
            "unexpected query plan: {plan:?}"
        );
    }

    #[tokio::test]
    async fn synced_message_count_uses_trigger_maintained_folder_totals() {
        let db = test_db().await;
        sqlx::query(
            "INSERT INTO folders (id, account_id, full_path, folder_type, sync_enabled) VALUES
             ('enabled', 'account-a', 'INBOX', 'INBOX', 1),
             ('disabled', 'account-a', 'Archive', 'ARCHIVE', 0),
             ('other', 'account-b', 'INBOX', 'INBOX', 1)",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages
             (id, account_id, folder_id, uid, internal_date, is_deleted) VALUES
             ('live', 'account-a', 'enabled', 1, '2026-01-01T00:00:00Z', 0),
             ('deleted', 'account-a', 'enabled', 2, '2026-01-02T00:00:00Z', 1),
             ('disabled-live', 'account-a', 'disabled', 3, '2026-01-03T00:00:00Z', 0),
             ('other-live', 'account-b', 'other', 4, '2026-01-04T00:00:00Z', 0)",
        )
        .execute(&db)
        .await
        .unwrap();

        let count: i64 = sqlx::query_scalar(SYNCED_MESSAGE_COUNT_SQL)
            .bind("account-a")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 1);

        let plan: Vec<(i64, i64, i64, String)> =
            sqlx::query_as(&format!("EXPLAIN QUERY PLAN {SYNCED_MESSAGE_COUNT_SQL}"))
                .bind("account-a")
                .fetch_all(&db)
                .await
                .unwrap();
        assert!(
            plan.iter()
                .all(|(_, _, _, detail)| !detail.contains("messages")),
            "synced-message count unexpectedly scanned messages: {plan:?}"
        );

        sqlx::query("UPDATE messages SET is_deleted = 1 WHERE id = 'live'")
            .execute(&db)
            .await
            .unwrap();
        let after_delete: i64 = sqlx::query_scalar(SYNCED_MESSAGE_COUNT_SQL)
            .bind("account-a")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(after_delete, 0);

        sqlx::query("UPDATE messages SET is_deleted = 0 WHERE id = 'live'")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("DELETE FROM messages WHERE id = 'live'")
            .execute(&db)
            .await
            .unwrap();
        let after_hard_delete: i64 = sqlx::query_scalar(SYNCED_MESSAGE_COUNT_SQL)
            .bind("account-a")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(after_hard_delete, 0);
    }

    #[tokio::test]
    async fn thread_unread_is_scoped_to_rendered_folder() {
        let db = test_db().await;
        sqlx::query(
            "INSERT INTO folders (id, account_id, full_path, folder_type) VALUES
             ('inbox', 'acc', 'INBOX', 'INBOX'),
             ('label', 'acc', 'Label_1', 'CUSTOM')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages
             (id, account_id, folder_id, uid, message_id_header, thread_id, subject, from_addr, internal_date, is_read)
             VALUES
             ('m1', 'acc', 'inbox', 1, '<same@example>', 'thread-a', 'Read copy', 'a@example.com', '2026-01-02T00:00:00Z', 1),
             ('m2', 'acc', 'label', 1, '<same@example>', 'thread-a', 'Unread label copy', 'a@example.com', '2026-01-01T00:00:00Z', 0)",
        )
        .execute(&db)
        .await
        .unwrap();

        let page = unified_page(&db, Some("inbox"), Some("acc"), None, 50, false)
            .await
            .unwrap();

        assert_eq!(page.items.len(), 1);
        assert!(page.items[0].is_read);
        assert_eq!(page.items[0].thread_unread, 0);
    }

    #[tokio::test]
    async fn local_draft_is_returned_by_the_unified_drafts_view() {
        let db = test_db().await;
        sqlx::query(
            "INSERT INTO folders (id, account_id, full_path, folder_type) VALUES
             ('drafts', 'acc', 'Mailquill Drafts', 'DRAFTS')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages
             (id, account_id, folder_id, uid, thread_id, subject, from_addr, internal_date, is_read, is_local_draft)
             VALUES ('draft-1', 'acc', 'drafts', -1, 'draft-1', 'Unfinished', 'me@example.test', '2026-01-01T00:00:00Z', 1, 1)",
        )
        .execute(&db)
        .await
        .unwrap();

        let page = unified_page(&db, Some("drafts"), None, None, 50, false)
            .await
            .unwrap();

        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        assert!(page.items[0].is_local_draft);
    }
}
