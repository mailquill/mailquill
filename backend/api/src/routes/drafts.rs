use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use bytes::Bytes;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{error::AppError, middleware::UserId, state::AppState};

const MAX_ATTACHMENT_BYTES: usize = 25 * 1024 * 1024;

#[derive(Clone, Deserialize, Serialize)]
pub struct DraftAttachment {
    filename: String,
    content_type: String,
    data: String,
}

#[derive(Deserialize)]
pub struct SaveDraftRequest {
    draft_id: Option<String>,
    account_id: String,
    from: String,
    #[serde(default)]
    to: Vec<String>,
    #[serde(default)]
    cc: Vec<String>,
    #[serde(default)]
    bcc: Vec<String>,
    #[serde(default)]
    subject: String,
    body_text: Option<String>,
    body_html: Option<String>,
    #[serde(default)]
    attachments: Vec<DraftAttachment>,
    in_reply_to: Option<String>,
    references: Option<String>,
}

/// Create or update a provider-neutral draft in the user's local mailbox.
pub async fn save_draft(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<SaveDraftRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    require_sender_identity(&user_db, &req.account_id, &req.from).await?;

    let attachment_bytes: usize = req
        .attachments
        .iter()
        .map(|attachment| attachment.data.len() * 3 / 4)
        .sum();
    if attachment_bytes > MAX_ATTACHMENT_BYTES {
        return Err(AppError::Unprocessable(
            "total attachment size exceeds 25 MB limit".into(),
        ));
    }

    let draft_id = req
        .draft_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if req.draft_id.is_some() {
        let owned: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM messages WHERE id = ? AND account_id = ? AND is_local_draft = 1)",
        )
        .bind(&draft_id)
        .bind(&req.account_id)
        .fetch_one(&user_db)
        .await?;
        if !owned {
            return Err(AppError::NotFound);
        }
    }

    let folder_id = ensure_drafts_folder(&user_db, &req.account_id).await?;
    let body_json = serde_json::to_vec(&json!({
        "html": req.body_html,
        "text": req.body_text,
    }))
    .map_err(|error| AppError::Internal(error.to_string()))?;
    let compressed = mailquill_core::compression::compress_body(&body_json);
    let blob_key = format!("drafts/{}/{draft_id}/body", req.account_id);
    state
        .blob_store
        .put(&blob_key, Bytes::from(compressed.clone()))
        .await
        .map_err(|error| AppError::Internal(error.to_string()))?;

    let now = Utc::now().to_rfc3339();
    let snippet_source = req
        .body_text
        .as_deref()
        .or(req.body_html.as_deref())
        .unwrap_or_default();
    let snippet: String = snippet_source.chars().take(200).collect();
    let mut transaction = user_db.begin().await?;
    if req.draft_id.is_some() {
        sqlx::query(
            "UPDATE messages SET folder_id = ?, subject = ?, subject_normalized = LOWER(?), snippet = ?, from_addr = ?, to_addrs = ?, cc_addrs = ?, in_reply_to = ?, \"references\" = ?, internal_date = ?, synced_at = ? WHERE id = ? AND is_local_draft = 1",
        )
        .bind(&folder_id)
        .bind(&req.subject)
        .bind(&req.subject)
        .bind(&snippet)
        .bind(&req.from)
        .bind(req.to.join(", "))
        .bind(req.cc.join(", "))
        .bind(&req.in_reply_to)
        .bind(&req.references)
        .bind(&now)
        .bind(&now)
        .bind(&draft_id)
        .execute(&mut *transaction)
        .await?;
    } else {
        let uid: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MIN(uid), 0) - 1 FROM messages WHERE folder_id = ? AND uid <= 0",
        )
        .bind(&folder_id)
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO messages (id, account_id, folder_id, uid, thread_id, in_reply_to, \"references\", subject, subject_normalized, snippet, from_addr, to_addrs, cc_addrs, internal_date, is_read, phishing_verdict, is_local_draft) VALUES (?, ?, ?, ?, ?, ?, ?, ?, LOWER(?), ?, ?, ?, ?, ?, 1, 'clean', 1)",
        )
        .bind(&draft_id)
        .bind(&req.account_id)
        .bind(&folder_id)
        .bind(uid)
        .bind(&draft_id)
        .bind(&req.in_reply_to)
        .bind(&req.references)
        .bind(&req.subject)
        .bind(&req.subject)
        .bind(&snippet)
        .bind(&req.from)
        .bind(req.to.join(", "))
        .bind(req.cc.join(", "))
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
    }

    sqlx::query(
        "INSERT INTO message_bodies (message_id, blob_key, size_bytes, size_bytes_uncompressed) VALUES (?, ?, ?, ?) ON CONFLICT(message_id) DO UPDATE SET blob_key = excluded.blob_key, size_bytes = excluded.size_bytes, size_bytes_uncompressed = excluded.size_bytes_uncompressed, fetched_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
    )
    .bind(&draft_id)
    .bind(&blob_key)
    .bind(compressed.len() as i64)
    .bind(body_json.len() as i64)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO local_drafts (message_id, to_addrs_json, cc_addrs_json, bcc_addrs_json, attachments_json) VALUES (?, ?, ?, ?, ?) ON CONFLICT(message_id) DO UPDATE SET to_addrs_json = excluded.to_addrs_json, cc_addrs_json = excluded.cc_addrs_json, bcc_addrs_json = excluded.bcc_addrs_json, attachments_json = excluded.attachments_json, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
    )
    .bind(&draft_id)
    .bind(serde_json::to_string(&req.to).map_err(|error| AppError::Internal(error.to_string()))?)
    .bind(serde_json::to_string(&req.cc).map_err(|error| AppError::Internal(error.to_string()))?)
    .bind(serde_json::to_string(&req.bcc).map_err(|error| AppError::Internal(error.to_string()))?)
    .bind(serde_json::to_string(&req.attachments).map_err(|error| AppError::Internal(error.to_string()))?)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(Json(json!({ "id": draft_id })))
}

/// Delete a local draft and its stored body.
pub async fn delete_draft(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(draft_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    delete_local_draft(&state, &user_db, &draft_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn delete_local_draft(
    state: &AppState,
    user_db: &sqlx::SqlitePool,
    draft_id: &str,
) -> Result<(), AppError> {
    let blob_key: Option<String> = sqlx::query_scalar(
        "SELECT b.blob_key FROM message_bodies b JOIN messages m ON m.id = b.message_id WHERE m.id = ? AND m.is_local_draft = 1",
    )
    .bind(&draft_id)
    .fetch_optional(user_db)
    .await?;
    let result = sqlx::query("DELETE FROM messages WHERE id = ? AND is_local_draft = 1")
        .bind(&draft_id)
        .execute(user_db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    if let Some(key) = blob_key {
        let _ = state.blob_store.delete(&key).await;
    }
    Ok(())
}

async fn ensure_drafts_folder(db: &sqlx::SqlitePool, account_id: &str) -> Result<String, AppError> {
    if let Some(id) = sqlx::query_scalar::<_, String>(
        "SELECT id FROM folders WHERE account_id = ? AND folder_type = 'DRAFTS' ORDER BY sync_enabled DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(db)
    .await?
    {
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO folders (id, account_id, name, full_path, folder_type, sync_enabled) VALUES (?, ?, 'Drafts', 'Mailquill Drafts', 'DRAFTS', 0)")
        .bind(&id)
        .bind(account_id)
        .execute(db)
        .await?;
    Ok(id)
}

async fn require_sender_identity(
    db: &sqlx::SqlitePool,
    account_id: &str,
    from: &str,
) -> Result<(), AppError> {
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM email_accounts WHERE id = ? AND LOWER(primary_email) = LOWER(?) UNION ALL SELECT 1 FROM account_aliases WHERE account_id = ? AND LOWER(email) = LOWER(?))",
    )
    .bind(account_id)
    .bind(from)
    .bind(account_id)
    .bind(from)
    .fetch_one(db)
    .await?;
    if allowed {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}
