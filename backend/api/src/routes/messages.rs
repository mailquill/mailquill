use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{error::AppError, middleware::UserId, state::AppState};

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: String,
    account_id: String,
    folder_id: String,
    uid: i64,
    message_id_header: Option<String>,
    thread_id: Option<String>,
    in_reply_to: Option<String>,
    references: Option<String>,
    list_id: Option<String>,
    subject: String,
    from_addr: String,
    to_addrs: String,
    cc_addrs: String,
    snippet: String,
    date: Option<String>,
    internal_date: String,
    is_read: bool,
    is_flagged: bool,
    is_deleted: bool,
}

#[derive(Deserialize)]
pub struct MoveRequest {
    folder_id: String,
}

#[derive(Deserialize)]
pub struct ReadRequest {
    is_read: bool,
}

#[derive(Deserialize)]
pub struct FlagRequest {
    is_flagged: bool,
}

pub async fn get_message(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(message_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    let row: Option<MessageRow> = sqlx::query_as(
        "SELECT id, account_id, folder_id, uid, message_id_header, thread_id, in_reply_to, \
         \"references\", list_id, subject, from_addr, to_addrs, cc_addrs, snippet, date, \
         internal_date, is_read, is_flagged, is_deleted FROM messages WHERE id = ?",
    )
    .bind(&message_id)
    .fetch_optional(&user_db)
    .await?;

    let row = row.ok_or(AppError::NotFound)?;

    // Check if body is available (task 4.10/4.11)
    let body_row: Option<(String, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT blob_key, size_bytes, size_bytes_uncompressed FROM message_bodies WHERE message_id = ?",
    )
    .bind(&message_id)
    .fetch_optional(&user_db)
    .await?;

    let (body_html, body_text, body_available) = if let Some((blob_key, _size, _size_uncompressed)) = body_row {
        // Read body from blob store and decompress
        match state.blob_store.get(&blob_key).await {
            Ok(compressed) => {
                match mailquill_core::compression::decompress_body(&compressed) {
                    Ok(decompressed) => {
                        // Parse as JSON to get html/text parts
                        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&decompressed) {
                            let html = v["html"].as_str().map(|s| s.to_owned());
                            let text = v["text"].as_str().map(|s| s.to_owned());
                            (html, text, true)
                        } else {
                            // Raw bytes — treat as text
                            let text = String::from_utf8_lossy(&decompressed).into_owned();
                            (None, Some(text), true)
                        }
                    }
                    Err(_) => (None, None, false),
                }
            }
            Err(_) => {
                // Body not in blob store yet — trigger on-demand IMAP fetch
                match fetch_body_on_demand(&state, &user.0, &user_db, &message_id, row.account_id.clone(), row.folder_id.clone(), row.uid as u32).await {
                    Ok((html, text)) => {
                        // Mark as read after successful body load (task 4.11)
                        let _ = sqlx::query("UPDATE messages SET is_read = 1 WHERE id = ?")
                            .bind(&message_id)
                            .execute(&user_db)
                            .await;
                        queue_imap_flag(&state, &user.0, &user_db, &message_id, "seen", true).await;
                        (html, text, true)
                    }
                    Err(_) => (None, None, false),
                }
            }
        }
    } else {
        // No body record — trigger on-demand fetch
        match fetch_body_on_demand(&state, &user.0, &user_db, &message_id, row.account_id.clone(), row.folder_id.clone(), row.uid as u32).await {
            Ok((html, text)) => {
                // Mark as read after successful body load
                let _ = sqlx::query("UPDATE messages SET is_read = 1 WHERE id = ?")
                    .bind(&message_id)
                    .execute(&user_db)
                    .await;
                queue_imap_flag(&state, &user.0, &user_db, &message_id, "seen", true).await;
                (html, text, true)
            }
            Err(_) => (None, None, false),
        }
    };

    // Thread summary
    let (thread_size, thread_unread) = if let Some(ref tid) = row.thread_id {
        let size: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE thread_id = ? AND is_deleted = 0")
            .bind(tid).fetch_one(&user_db).await.unwrap_or(1);
        let unread: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE thread_id = ? AND is_read = 0 AND is_deleted = 0")
            .bind(tid).fetch_one(&user_db).await.unwrap_or(0);
        (size, unread)
    } else {
        (1, if row.is_read { 0 } else { 1 })
    };

    // Attachments
    let attachments: Vec<(String, Option<String>, String, Option<i64>)> = sqlx::query_as(
        "SELECT id, filename, content_type, size_bytes FROM attachments WHERE message_id = ?",
    )
    .bind(&message_id)
    .fetch_all(&user_db)
    .await
    .unwrap_or_default();

    Ok(Json(json!({
        "id": row.id,
        "account_id": row.account_id,
        "folder_id": row.folder_id,
        "uid": row.uid,
        "message_id_header": row.message_id_header,
        "thread_id": row.thread_id,
        "in_reply_to": row.in_reply_to,
        "references": row.references,
        "list_id": row.list_id,
        "subject": row.subject,
        "from_addr": row.from_addr,
        "to_addrs": row.to_addrs,
        "cc_addrs": row.cc_addrs,
        "snippet": row.snippet,
        "date": row.date,
        "internal_date": row.internal_date,
        "is_read": row.is_read,
        "is_flagged": row.is_flagged,
        "is_deleted": row.is_deleted,
        "body_html": body_html,
        "body_text": body_text,
        "body_available": body_available,
        "thread_size": thread_size,
        "thread_unread": thread_unread,
        "attachments": attachments.into_iter().map(|(id, filename, content_type, size)| json!({
            "id": id,
            "filename": filename,
            "content_type": content_type,
            "size_bytes": size,
        })).collect::<Vec<_>>(),
    })))
}

pub async fn mark_read(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(message_id): Path<String>,
    Json(req): Json<ReadRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    require_message_exists(&user_db, &message_id).await?;

    sqlx::query("UPDATE messages SET is_read = ? WHERE id = ?")
        .bind(req.is_read)
        .bind(&message_id)
        .execute(&user_db)
        .await?;

    // Queue IMAP flag update
    queue_imap_flag(&state, &user.0, &user_db, &message_id, "seen", req.is_read).await;

    Ok(Json(json!({ "ok": true })))
}

pub async fn toggle_flag(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(message_id): Path<String>,
    Json(req): Json<FlagRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    require_message_exists(&user_db, &message_id).await?;

    sqlx::query("UPDATE messages SET is_flagged = ? WHERE id = ?")
        .bind(req.is_flagged)
        .bind(&message_id)
        .execute(&user_db)
        .await?;

    queue_imap_flag(&state, &user.0, &user_db, &message_id, "flagged", req.is_flagged).await;

    Ok(Json(json!({ "ok": true })))
}

pub async fn archive_message(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(message_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let (account_id, uid, folder_full_path) = get_message_location(&user_db, &message_id).await?;

    // Queue IMAP MOVE to Archive
    state.sync_manager.queue_imap_move(
        user.0.clone(),
        account_id,
        uid,
        folder_full_path,
        "Archive".into(),
        false,
    ).await;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_message(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(message_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let (account_id, uid, folder_full_path) = get_message_location(&user_db, &message_id).await?;

    let is_trash = folder_full_path.to_lowercase().contains("trash")
        || folder_full_path.to_lowercase().contains("deleted");

    if is_trash {
        // Hard delete — EXPUNGE
        state.sync_manager.queue_imap_expunge(
            user.0.clone(),
            account_id,
            uid,
            folder_full_path,
        ).await;
    } else {
        // Soft delete — move to Trash
        state.sync_manager.queue_imap_move(
            user.0.clone(),
            account_id,
            uid,
            folder_full_path,
            "Trash".into(),
            false,
        ).await;
    }

    sqlx::query("UPDATE messages SET is_deleted = 1 WHERE id = ?")
        .bind(&message_id)
        .execute(&user_db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn move_message(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(message_id): Path<String>,
    Json(req): Json<MoveRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let (account_id, uid, src_folder) = get_message_location(&user_db, &message_id).await?;

    // Get destination folder path
    let dest_path: Option<String> =
        sqlx::query_scalar("SELECT full_path FROM folders WHERE id = ?")
            .bind(&req.folder_id)
            .fetch_optional(&user_db)
            .await?;
    let dest_path = dest_path.ok_or(AppError::NotFound)?;

    state.sync_manager.queue_imap_move(
        user.0.clone(),
        account_id,
        uid,
        src_folder,
        dest_path,
        false,
    ).await;

    Ok(StatusCode::NO_CONTENT)
}

// ── helpers ───────────────────────────────────────────────────────────────────

async fn require_message_exists(db: &sqlx::SqlitePool, message_id: &str) -> Result<(), AppError> {
    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM messages WHERE id = ?")
            .bind(message_id)
            .fetch_optional(db)
            .await?;
    exists.map(|_| ()).ok_or(AppError::NotFound)
}

async fn get_message_location(
    db: &sqlx::SqlitePool,
    message_id: &str,
) -> Result<(String, u32, String), AppError> {
    let row: Option<(String, i64, String)> = sqlx::query_as(
        "SELECT m.account_id, m.uid, f.full_path FROM messages m JOIN folders f ON f.id = m.folder_id WHERE m.id = ?",
    )
    .bind(message_id)
    .fetch_optional(db)
    .await?;

    let (account_id, uid, folder_path) = row.ok_or(AppError::NotFound)?;
    Ok((account_id, uid as u32, folder_path))
}

async fn fetch_body_on_demand(
    state: &AppState,
    user_id: &str,
    user_db: &sqlx::SqlitePool,
    message_id: &str,
    account_id: String,
    folder_id: String,
    uid: u32,
) -> Result<(Option<String>, Option<String>), AppError> {
    // Get account credentials
    let row: Option<(Vec<u8>, String, i64, String)> = sqlx::query_as(
        "SELECT credentials_encrypted, imap_host, imap_port, imap_auth_scheme FROM email_accounts WHERE id = ?",
    )
    .bind(&account_id)
    .fetch_optional(user_db)
    .await?;

    let (creds_enc, host, port, auth_scheme) = row.ok_or(AppError::NotFound)?;
    let creds_bytes = state
        .credential_key
        .decrypt(&creds_enc)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let creds: serde_json::Value =
        serde_json::from_slice(&creds_bytes).map_err(|e| AppError::Internal(e.to_string()))?;

    // Get folder path
    let folder_path: Option<String> =
        sqlx::query_scalar("SELECT full_path FROM folders WHERE id = ?")
            .bind(&folder_id)
            .fetch_optional(user_db)
            .await?;
    let folder_path = folder_path.ok_or(AppError::NotFound)?;

    let imap_user = creds["imap_username"].as_str().unwrap_or("").to_owned();
    let imap_pass = creds["imap_password"].as_str().unwrap_or("").to_owned();
    let oauth_token = creds["oauth_access_token"].as_str().map(|s| s.to_owned());

    let result = imap_sync::fetch_body_by_uid(
        &host,
        port as u16,
        &imap_user,
        &imap_pass,
        oauth_token.as_deref(),
        &auth_scheme,
        &folder_path,
        uid,
        state.blob_store.clone(),
        &account_id,
        &folder_id,
        message_id,
        user_id,
        user_db,
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(result)
}

pub async fn queue_imap_flag(
    state: &AppState,
    user_id: &str,
    db: &sqlx::SqlitePool,
    message_id: &str,
    flag: &str,
    set: bool,
) {
    if let Ok(Some((account_id, uid, folder_path))) = async {
        let r: Result<Option<(String, i64, String)>, sqlx::Error> = sqlx::query_as(
            "SELECT m.account_id, m.uid, f.full_path FROM messages m JOIN folders f ON f.id = m.folder_id WHERE m.id = ?",
        )
        .bind(message_id)
        .fetch_optional(db)
        .await;
        r
    }.await {
        state.sync_manager.queue_imap_flag(
            user_id.to_owned(),
            account_id,
            uid as u32,
            folder_path,
            flag.to_owned(),
            set,
        ).await;
    }
}
