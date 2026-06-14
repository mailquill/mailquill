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
    /// Cross-account view: inbox (default), starred, sent, drafts, archive,
    /// spam, trash.
    view: Option<String>,
    /// Optional: scope the view to a single account (per-mailbox starred list).
    account_id: Option<String>,
}

pub async fn unified_inbox(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(q): Query<PaginationQuery>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let limit = q.limit.unwrap_or(PAGE_SIZE).min(100);

    let page = db::queries::unified_page(
        &user_db,
        q.view.as_deref(),
        q.account_id.as_deref(),
        q.cursor.as_deref(),
        limit,
    )
    .await?;

    Ok(Json(json!({
        "items": page.items,
        "total": page.total,
        "next_cursor": page.next_cursor,
    })))
}

/// Unread/flagged counts per cross-account view, for the unified sidebar badges.
pub async fn unified_counts(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    // Unread totals grouped by folder type across all accounts.
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT folder_type, COALESCE(SUM(unread_count), 0) FROM folders GROUP BY folder_type",
    )
    .fetch_all(&user_db)
    .await?;
    let unread = |t: &str| rows.iter().find(|(ft, _)| ft == t).map(|(_, c)| *c).unwrap_or(0);

    let starred: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE is_flagged = 1 AND is_deleted = 0")
            .fetch_one(&user_db)
            .await
            .unwrap_or(0);

    Ok(Json(json!({
        "inbox": unread("INBOX"),
        "starred": starred,
        "sent": unread("SENT"),
        "drafts": unread("DRAFTS"),
        "archive": unread("ARCHIVE"),
        "spam": unread("SPAM"),
        "trash": unread("TRASH"),
    })))
}

#[derive(Deserialize)]
pub struct BulkActionRequest {
    /// "read" | "archive" | "delete"
    action: String,
    /// Unified view scope (inbox/starred/sent/…). Used when no folder is given.
    view: Option<String>,
    /// Single-folder scope.
    account_id: Option<String>,
    folder: Option<String>,
}

/// Server-side bulk action over a whole view or folder — the "select all N"
/// path, so the client never enumerates thousands of ids. The local state
/// change is a single UPDATE (RETURNING the affected rows); IMAP propagation is
/// queued in a background task so the response stays fast.
pub async fn bulk_action(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<BulkActionRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    // Resolve the scope into a WHERE predicate plus its bind values.
    let (where_sql, binds): (String, Vec<String>) =
        if let (Some(account_id), Some(folder)) = (&req.account_id, &req.folder) {
            let fid: Option<String> = sqlx::query_scalar(
                "SELECT id FROM folders WHERE account_id = ? AND full_path = ? COLLATE NOCASE LIMIT 1",
            )
            .bind(account_id)
            .bind(folder)
            .fetch_optional(&user_db)
            .await?;
            (
                "folder_id = ? AND is_deleted = 0".into(),
                vec![fid.ok_or(AppError::NotFound)?],
            )
        } else {
            let (mut pred, mut binds): (String, Vec<String>) =
                match req.view.as_deref().unwrap_or("inbox") {
                    "starred" => ("is_flagged = 1 AND is_deleted = 0".into(), vec![]),
                    v => {
                        let ft = match v {
                            "sent" => "SENT",
                            "drafts" => "DRAFTS",
                            "archive" => "ARCHIVE",
                            "spam" => "SPAM",
                            "trash" => "TRASH",
                            _ => "INBOX",
                        };
                        (
                            "folder_id IN (SELECT id FROM folders WHERE folder_type = ?) AND is_deleted = 0".into(),
                            vec![ft.to_string()],
                        )
                    }
                };
            // Scope a view to one account (per-mailbox starred select-all) so the
            // action never spills over to other accounts' messages.
            if let Some(account_id) = &req.account_id {
                pred.push_str(" AND account_id = ?");
                binds.push(account_id.clone());
            }
            (pred, binds)
        };

    let set_sql = match req.action.as_str() {
        "read" => "is_read = 1",
        "flag" => "is_flagged = 1",
        // Archive and delete both hide the rows locally now; IMAP MOVE (queued
        // below) and the next sync reconcile their real location.
        "archive" | "delete" => "is_deleted = 1",
        _ => return Err(AppError::Unprocessable("invalid action".into())),
    };

    let sql =
        format!("UPDATE messages SET {set_sql} WHERE {where_sql} RETURNING account_id, uid, folder_id");
    let mut q = sqlx::query_as::<_, (String, i64, String)>(&sql);
    for b in &binds {
        q = q.bind(b);
    }
    let affected = q.fetch_all(&user_db).await?;
    let affected_len = affected.len();

    crate::routes::messages::refresh_unread_counts(&user_db).await;

    // Propagate to IMAP in the background — queueing potentially thousands of
    // commands must not block the response.
    let action = req.action.clone();
    let state2 = state.clone();
    let user_id = user.0.clone();
    let db2 = user_db.clone();
    tokio::spawn(async move {
        let mut paths: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for (account_id, uid, folder_id) in affected {
            let full_path = match paths.get(&folder_id) {
                Some(p) => p.clone(),
                None => {
                    let p: String =
                        sqlx::query_scalar("SELECT full_path FROM folders WHERE id = ?")
                            .bind(&folder_id)
                            .fetch_one(&db2)
                            .await
                            .unwrap_or_default();
                    paths.insert(folder_id.clone(), p.clone());
                    p
                }
            };
            let uid = uid as u32;
            match action.as_str() {
                "read" => {
                    state2
                        .sync_manager
                        .queue_imap_flag(user_id.clone(), account_id, uid, full_path, "seen".into(), true)
                        .await
                }
                "flag" => {
                    state2
                        .sync_manager
                        .queue_imap_flag(user_id.clone(), account_id, uid, full_path, "flagged".into(), true)
                        .await
                }
                "archive" => {
                    state2
                        .sync_manager
                        .queue_imap_move(user_id.clone(), account_id, uid, full_path, "Archive".into(), false)
                        .await
                }
                "delete" => {
                    let lower = full_path.to_lowercase();
                    if lower.contains("trash") || lower.contains("deleted") {
                        state2
                            .sync_manager
                            .queue_imap_expunge(user_id.clone(), account_id, uid, full_path)
                            .await
                    } else {
                        state2
                            .sync_manager
                            .queue_imap_move(user_id.clone(), account_id, uid, full_path, "Trash".into(), false)
                            .await
                    }
                }
                _ => {}
            }
        }
    });

    Ok(Json(json!({ "affected": affected_len })))
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

    let rows: Vec<(String, String, String, String, i64, i64)> = sqlx::query_as(
        "SELECT id, name, full_path, folder_type, unread_count, sync_enabled FROM folders WHERE account_id = ? ORDER BY folder_type, full_path",
    )
    .bind(&account_id)
    .fetch_all(&user_db)
    .await?;

    let folders: Vec<_> = rows
        .into_iter()
        .map(|(id, name, full_path, folder_type, unread_count, sync_enabled)| {
            // `full_path` stays the raw IMAP identifier (used for commands and
            // routing); `folder_name`/`folder_name_server` carry the decoded and
            // raw leaf names for display.
            let raw_leaf = full_path.rsplit('/').next().unwrap_or(&full_path).to_string();
            json!({
                "id": id,
                "name": name,
                "full_path": full_path,
                "folder_name": crate::imap_utf7::decode(&raw_leaf),
                "folder_name_server": raw_leaf,
                "folder_type": folder_type,
                "unread_count": unread_count,
                "sync_enabled": sync_enabled != 0,
                "account_id": account_id,
            })
        })
        .collect();

    Ok(Json(folders))
}

#[derive(Deserialize)]
pub struct FolderSyncRequest {
    sync_enabled: bool,
}

/// Toggle whether a folder is synced. Disabling stops future syncs for it (its
/// already-fetched messages stay); enabling triggers an immediate poll so it
/// backfills now instead of waiting for the next cycle.
pub async fn set_folder_sync(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path((account_id, folder_path)): Path<(String, String)>,
    Json(req): Json<FolderSyncRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let folder_path = urlencoding::decode(&folder_path)
        .unwrap_or_else(|_| std::borrow::Cow::Borrowed(&folder_path))
        .into_owned();

    let res = sqlx::query("UPDATE folders SET sync_enabled = ? WHERE account_id = ? AND full_path = ?")
        .bind(req.sync_enabled as i64)
        .bind(&account_id)
        .bind(&folder_path)
        .execute(&user_db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    if req.sync_enabled {
        state.sync_manager.force_poll(&account_id).await;
    }

    Ok(Json(json!({ "sync_enabled": req.sync_enabled })))
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

    let exact: Option<String> = sqlx::query_scalar(
        "SELECT id FROM folders WHERE account_id = ? AND full_path = ?",
    )
    .bind(&account_id)
    .bind(&folder_path)
    .fetch_optional(&user_db)
    .await?;
    // Fall back to a case-insensitive match so a bookmarked/typed `inbox`
    // resolves to the canonical `INBOX` folder instead of 404ing.
    let folder_id = match exact {
        Some(id) => id,
        None => sqlx::query_scalar(
            "SELECT id FROM folders WHERE account_id = ? AND full_path = ? COLLATE NOCASE LIMIT 1",
        )
        .bind(&account_id)
        .bind(&folder_path)
        .fetch_optional(&user_db)
        .await?
        .ok_or(AppError::NotFound)?,
    };

    let page = db::queries::folder_page(&user_db, &folder_id, q.cursor.as_deref(), limit).await?;

    Ok(Json(json!({
        "account_id": account_id,
        "folder_path": folder_path,
        "items": page.items,
        "total": page.total,
        "next_cursor": page.next_cursor,
    })))
}
