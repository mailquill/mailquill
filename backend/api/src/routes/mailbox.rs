use axum::{
    extract::{Extension, Path, Query, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{error::AppError, middleware::UserId, state::AppState};

const PAGE_SIZE: i64 = 50;

#[derive(Deserialize)]
pub struct PaginationQuery {
    cursor: Option<String>,
    limit: Option<i64>,
    #[serde(rename = "folder")]
    _folder: Option<String>,
}

pub async fn unified_inbox(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(q): Query<PaginationQuery>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let limit = q.limit.unwrap_or(PAGE_SIZE).min(100);

    // Cursor = internal_date of last item (ISO string)
    let cursor_clause = if let Some(ref c) = q.cursor {
        format!("AND m.internal_date < '{}'", c.replace('\'', "''"))
    } else {
        String::new()
    };

    // Get INBOX folders across all accounts
    let sql = format!(
        "SELECT m.id, m.thread_id, m.subject, m.from_addr, m.snippet, m.internal_date, m.is_read, m.is_flagged, m.account_id, m.folder_id, m.list_id FROM messages m JOIN folders f ON f.id = m.folder_id WHERE f.folder_type = 'INBOX' AND m.is_deleted = 0 {cursor_clause} ORDER BY m.internal_date DESC LIMIT ?",
    );

    let rows: Vec<(String, Option<String>, String, String, String, String, bool, bool, String, String, Option<String>)> = sqlx::query_as(&sql)
        .bind(limit)
        .fetch_all(&user_db)
        .await?;

    let next_cursor = rows.last().map(|r| r.5.clone());
    let has_more = rows.len() as i64 == limit;

    let items = build_thread_rows(&user_db, rows).await;

    Ok(Json(json!({
        "items": items,
        "next_cursor": if has_more { next_cursor } else { None::<String> },
    })))
}

pub async fn list_folders(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    // Check account exists
    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM email_accounts WHERE id = ?")
            .bind(&account_id)
            .fetch_optional(&user_db)
            .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    let rows: Vec<(String, String, String, String, i64)> = sqlx::query_as(
        "SELECT id, name, full_path, folder_type, unread_count FROM folders WHERE account_id = ? ORDER BY folder_type, full_path",
    )
    .bind(&account_id)
    .fetch_all(&user_db)
    .await?;

    let folders: Vec<_> = rows
        .into_iter()
        .map(|(id, name, full_path, folder_type, unread_count)| {
            json!({
                "id": id,
                "name": name,
                "full_path": full_path,
                "folder_type": folder_type,
                "unread_count": unread_count,
                "account_id": account_id,
            })
        })
        .collect();

    Ok(Json(folders))
}

pub async fn list_folder_messages(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path((account_id, folder_path)): Path<(String, String)>,
    Query(q): Query<PaginationQuery>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let limit = q.limit.unwrap_or(PAGE_SIZE).min(100);

    // Decode URL-encoded folder path
    let folder_path = urlencoding::decode(&folder_path)
        .unwrap_or_else(|_| std::borrow::Cow::Borrowed(&folder_path))
        .into_owned();

    let folder_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM folders WHERE account_id = ? AND full_path = ?",
    )
    .bind(&account_id)
    .bind(&folder_path)
    .fetch_optional(&user_db)
    .await?;
    let folder_id = folder_id.ok_or(AppError::NotFound)?;

    let cursor_clause = if let Some(ref c) = q.cursor {
        format!("AND m.internal_date < '{}'", c.replace('\'', "''"))
    } else {
        String::new()
    };

    let sql = format!(
        "SELECT m.id, m.thread_id, m.subject, m.from_addr, m.snippet, m.internal_date, m.is_read, m.is_flagged, m.account_id, m.folder_id, m.list_id FROM messages m WHERE m.folder_id = ? AND m.is_deleted = 0 {cursor_clause} ORDER BY m.internal_date DESC LIMIT ?",
    );

    let rows: Vec<(String, Option<String>, String, String, String, String, bool, bool, String, String, Option<String>)> = sqlx::query_as(&sql)
        .bind(&folder_id)
        .bind(limit)
        .fetch_all(&user_db)
        .await?;

    let next_cursor = rows.last().map(|r| r.5.clone());
    let has_more = rows.len() as i64 == limit;

    let items = build_thread_rows(&user_db, rows).await;

    Ok(Json(json!({
        "account_id": account_id,
        "folder_path": folder_path,
        "items": items,
        "next_cursor": if has_more { next_cursor } else { None::<String> },
    })))
}

// ── helpers ───────────────────────────────────────────────────────────────────

async fn build_thread_rows(
    db: &sqlx::SqlitePool,
    rows: Vec<(String, Option<String>, String, String, String, String, bool, bool, String, String, Option<String>)>,
) -> Vec<serde_json::Value> {
    let mut result = Vec::with_capacity(rows.len());

    for (msg_id, thread_id, subject, from_addr, snippet, internal_date, is_read, is_flagged, account_id, folder_id, list_id) in rows {
        let (thread_size, thread_unread, participants) = if let Some(ref tid) = thread_id {
            let size: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE thread_id = ? AND is_deleted = 0")
                .bind(tid).fetch_one(db).await.unwrap_or(1);
            let unread: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE thread_id = ? AND is_read = 0 AND is_deleted = 0")
                .bind(tid).fetch_one(db).await.unwrap_or(0);
            let p: Vec<String> = sqlx::query_scalar("SELECT DISTINCT from_addr FROM messages WHERE thread_id = ? AND is_deleted = 0 ORDER BY internal_date ASC LIMIT 3")
                .bind(tid).fetch_all(db).await.unwrap_or_default();
            (size, unread, p)
        } else {
            (1, if is_read { 0 } else { 1 }, vec![from_addr.clone()])
        };

        result.push(json!({
            "message_id": msg_id,
            "thread_id": thread_id,
            "subject": subject,
            "from_addr": from_addr,
            "snippet": snippet,
            "internal_date": internal_date,
            "is_read": is_read,
            "is_flagged": is_flagged,
            "account_id": account_id,
            "folder_id": folder_id,
            "list_id": list_id,
            "thread_size": thread_size,
            "thread_unread": thread_unread,
            "thread_participants": participants,
        }));
    }

    result
}
