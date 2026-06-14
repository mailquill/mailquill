//! Mail list queries shared by the API handlers and the performance benchmark.
//!
//! Kept here (rather than inline in the axum handlers) so the hot path is
//! testable and benchmarkable in isolation. The thread enrichment is batched:
//! one aggregate query for size/unread and one ordered fetch for participants,
//! instead of an N+1 over the page.

use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

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
    pub folder_path: String,
    pub list_id: Option<String>,
    pub phishing_verdict: Option<String>,
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

const LIST_COLUMNS: &str = "m.id, m.thread_id, m.subject, m.from_addr, m.snippet, m.internal_date, m.is_read, m.is_flagged, m.account_id, m.folder_id, m.list_id, m.phishing_verdict, f.full_path";

/// Cross-account unified inbox/view page, newest first. Pass `account_id` to
/// scope the same view to a single mailbox (e.g. that account's starred list).
pub async fn unified_page(
    db: &SqlitePool,
    view: Option<&str>,
    account_id: Option<&str>,
    cursor: Option<&str>,
    limit: i64,
) -> Result<Page, sqlx::Error> {
    let filter = view_filter(view);
    let account_clause = if account_id.is_some() { "AND m.account_id = ?" } else { "" };
    let cursor_clause = if cursor.is_some() { "AND m.internal_date < ?" } else { "" };
    let sql = format!(
        "SELECT {LIST_COLUMNS} FROM messages m JOIN folders f ON f.id = m.folder_id WHERE {filter} AND m.is_deleted = 0 {account_clause} {cursor_clause} ORDER BY m.internal_date DESC LIMIT ?",
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

    let total = view_total(db, view, account_id).await;

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
async fn view_total(db: &SqlitePool, view: Option<&str>, account_id: Option<&str>) -> i64 {
    let account_clause = if account_id.is_some() { "AND account_id = ?" } else { "" };
    if view == Some("starred") {
        let sql = format!(
            "SELECT COUNT(*) FROM messages WHERE is_flagged = 1 AND is_deleted = 0 {account_clause}",
        );
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
    for fid in folder_ids {
        total += sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM messages WHERE folder_id = ? AND is_deleted = 0",
        )
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
) -> Result<Page, sqlx::Error> {
    let cursor_clause = if cursor.is_some() { "AND m.internal_date < ?" } else { "" };
    let sql = format!(
        "SELECT {LIST_COLUMNS} FROM messages m JOIN folders f ON f.id = m.folder_id WHERE m.folder_id = ? AND m.is_deleted = 0 {cursor_clause} ORDER BY m.internal_date DESC LIMIT ?",
    );
    let mut q = sqlx::query_as::<_, RawRow>(&sql).bind(folder_id);
    if let Some(c) = cursor {
        q = q.bind(c);
    }
    let rows = q.bind(limit).fetch_all(db).await?;

    let next_cursor = rows.last().map(|r| r.5.clone());
    let has_more = rows.len() as i64 == limit;

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder_id = ? AND is_deleted = 0")
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

    let mut counts: HashMap<String, (i64, i64)> = HashMap::new();
    let mut participants: HashMap<String, Vec<String>> = HashMap::new();

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
            "SELECT thread_id, COUNT(DISTINCT COALESCE(message_id_header, id)), SUM(CASE WHEN is_read = 0 THEN 1 ELSE 0 END) FROM messages INDEXED BY idx_msg_thread WHERE thread_id IN ({placeholders}) AND is_deleted = 0 GROUP BY thread_id",
        );
        let mut q = sqlx::query_as::<_, (String, i64, i64)>(&sql);
        for id in &thread_ids {
            q = q.bind(id);
        }
        for (tid, size, unread) in q.fetch_all(db).await? {
            counts.insert(tid, (size, unread));
        }

        let sql_p = format!(
            "SELECT thread_id, from_addr FROM messages INDEXED BY idx_msg_thread WHERE thread_id IN ({placeholders}) AND is_deleted = 0 ORDER BY internal_date ASC",
        );
        let mut qp = sqlx::query_as::<_, (String, String)>(&sql_p);
        for id in &thread_ids {
            qp = qp.bind(id);
        }
        for (tid, addr) in qp.fetch_all(db).await? {
            let v = participants.entry(tid).or_default();
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
            ) = r;

            let (thread_size, thread_unread, thread_participants) = match &thread_id {
                Some(tid) => {
                    let (size, unread) = counts
                        .get(tid)
                        .copied()
                        .unwrap_or((1, if is_read { 0 } else { 1 }));
                    let p = participants
                        .get(tid)
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
                folder_path,
                list_id,
                phishing_verdict,
                thread_size,
                thread_unread,
                thread_participants,
            }
        })
        .collect();

    Ok(items)
}
