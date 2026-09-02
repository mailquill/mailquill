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
    folder_type: Option<String>,
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
    is_local_draft: bool,
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
        "SELECT m.id, m.account_id, m.folder_id, f.folder_type, m.uid, m.message_id_header, m.thread_id, m.in_reply_to, \
         m.\"references\", m.list_id, m.subject, m.from_addr, m.to_addrs, m.cc_addrs, m.snippet, m.date, \
         m.internal_date, m.is_read, m.is_flagged, m.is_deleted, m.is_local_draft FROM messages m LEFT JOIN folders f ON f.id = m.folder_id WHERE m.id = ?",
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

    let (body_html, body_text, body_available) =
        if let Some((blob_key, _size, _size_uncompressed)) = body_row {
            // Read body from blob store and decompress
            match state.blob_store.get(&blob_key).await {
                Ok(compressed) => {
                    match mailquill_core::compression::decompress_body(&compressed) {
                        Ok(decompressed) => {
                            // Parse as JSON to get html/text parts
                            if let Ok(v) =
                                serde_json::from_slice::<serde_json::Value>(&decompressed)
                            {
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
                    match fetch_body_on_demand(
                        &state,
                        &user.0,
                        &user_db,
                        &message_id,
                        row.account_id.clone(),
                        row.folder_id.clone(),
                        row.uid as u32,
                    )
                    .await
                    {
                        Ok((html, text)) => (html, text, true),
                        Err(_) => (None, None, false),
                    }
                }
            }
        } else {
            // No body record — trigger on-demand fetch
            match fetch_body_on_demand(
                &state,
                &user.0,
                &user_db,
                &message_id,
                row.account_id.clone(),
                row.folder_id.clone(),
                row.uid as u32,
            )
            .await
            {
                Ok((html, text)) => (html, text, true),
                Err(_) => (None, None, false),
            }
        };

    // Phishing analysis: messages synced before the analyser existed (or in
    // lazy mode without an on-demand fetch above) have no verdict yet — fetch
    // the raw message once to backfill. analyse_and_store sets the verdict, so
    // this runs at most once per message.
    let verdict: Option<Option<String>> =
        sqlx::query_scalar("SELECT phishing_verdict FROM messages WHERE id = ?")
            .bind(&message_id)
            .fetch_optional(&user_db)
            .await?;
    if verdict.flatten().is_none() {
        let _ = fetch_body_on_demand(
            &state,
            &user.0,
            &user_db,
            &message_id,
            row.account_id.clone(),
            row.folder_id.clone(),
            row.uid as u32,
        )
        .await;
    }

    let phishing: Option<(i32, String, String)> = sqlx::query_as(
        "SELECT score, verdict, checks_json FROM phishing_analysis WHERE message_id = ?",
    )
    .bind(&message_id)
    .fetch_optional(&user_db)
    .await?;
    let (phishing_score, phishing_verdict, phishing_checks) = match phishing {
        Some((score, verdict, checks_json)) => (
            Some(score),
            Some(verdict),
            serde_json::from_str::<serde_json::Value>(&checks_json).unwrap_or_else(|_| json!([])),
        ),
        None => (None, None, json!([])),
    };

    // Inline images: the HTML references cid: parts. If no attachment with a
    // Content-ID is stored — either the body was fetched before attachment
    // storage existed (it sits in the blob store and the path above never
    // re-fetches), or it was synced before the content_id column — pull the
    // raw message once to backfill the attachment rows.
    let references_cid = body_html.as_deref().is_some_and(|h| h.contains("cid:"));
    if references_cid {
        let have_inline: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attachments WHERE message_id = ? AND content_id IS NOT NULL",
        )
        .bind(&message_id)
        .fetch_one(&user_db)
        .await
        .unwrap_or(0);
        if have_inline == 0 {
            let _ = fetch_body_on_demand(
                &state,
                &user.0,
                &user_db,
                &message_id,
                row.account_id.clone(),
                row.folder_id.clone(),
                row.uid as u32,
            )
            .await;
        }
    }

    // Thread summary
    let (thread_size, thread_unread) = if let Some(ref tid) = row.thread_id {
        let size: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT COALESCE(message_id_header, id)) FROM messages INDEXED BY idx_msg_thread_folder_identity_undeleted WHERE thread_id = ? AND folder_id = ? AND is_deleted = 0",
        )
        .bind(tid)
        .bind(&row.folder_id)
        .fetch_one(&user_db)
        .await
        .unwrap_or(1);
        let unread: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE thread_id = ? AND folder_id = ? AND is_read = 0 AND is_deleted = 0",
        )
        .bind(tid)
        .bind(&row.folder_id)
        .fetch_one(&user_db)
        .await
        .unwrap_or(0);
        (size, unread)
    } else {
        (1, if row.is_read { 0 } else { 1 })
    };

    // Attachments
    let attachments: Vec<(String, Option<String>, String, Option<String>, Option<i64>)> = sqlx::query_as(
        "SELECT id, filename, content_type, content_id, size_bytes FROM attachments WHERE message_id = ?",
    )
    .bind(&message_id)
    .fetch_all(&user_db)
    .await
    .unwrap_or_default();
    let local_draft: Option<(String, String, String, String)> = if row.is_local_draft {
        sqlx::query_as(
            "SELECT to_addrs_json, cc_addrs_json, bcc_addrs_json, attachments_json FROM local_drafts WHERE message_id = ?",
        )
        .bind(&message_id)
        .fetch_optional(&user_db)
        .await?
    } else {
        None
    };
    let (draft_to, draft_cc, draft_bcc, draft_attachments) = local_draft
        .map(|(to, cc, bcc, attachments)| {
            (
                serde_json::from_str::<serde_json::Value>(&to).unwrap_or_else(|_| json!([])),
                serde_json::from_str::<serde_json::Value>(&cc).unwrap_or_else(|_| json!([])),
                serde_json::from_str::<serde_json::Value>(&bcc).unwrap_or_else(|_| json!([])),
                serde_json::from_str::<serde_json::Value>(&attachments)
                    .unwrap_or_else(|_| json!([])),
            )
        })
        .unwrap_or_else(|| (json!([]), json!([]), json!([]), json!([])));

    Ok(Json(json!({
        "id": row.id,
        "account_id": row.account_id,
        "folder_id": row.folder_id,
        "folder_type": row.folder_type,
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
        "is_local_draft": row.is_local_draft,
        "body_html": body_html,
        "body_text": body_text,
        "body_available": body_available,
        "phishing_verdict": phishing_verdict,
        "phishing_score": phishing_score,
        "phishing_checks": phishing_checks,
        "thread_size": thread_size,
        "thread_unread": thread_unread,
        "attachments": attachments.into_iter().map(|(id, filename, content_type, content_id, size)| json!({
            "id": id,
            "filename": filename,
            "content_type": content_type,
            "content_id": content_id,
            "size_bytes": size,
        })).collect::<Vec<_>>(),
        "draft_to": draft_to,
        "draft_cc": draft_cc,
        "draft_bcc": draft_bcc,
        "draft_attachments": draft_attachments,
    })))
}

/// Raw attachment bytes with their content type — used for downloads and for
/// resolving inline cid: images in the reader.
pub async fn download_attachment(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(attachment_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    let row: Option<(String, String, Option<String>)> =
        sqlx::query_as("SELECT blob_key, content_type, filename FROM attachments WHERE id = ?")
            .bind(&attachment_id)
            .fetch_optional(&user_db)
            .await?;
    let (blob_key, content_type, filename) = row.ok_or(AppError::NotFound)?;

    let bytes = state
        .blob_store
        .get(&blob_key)
        .await
        .map_err(|_| AppError::NotFound)?;

    let disposition = match filename {
        Some(name) => format!("inline; filename=\"{}\"", name.replace('"', "")),
        None => "inline".to_owned(),
    };

    Ok((
        [
            (axum::http::header::CONTENT_TYPE, content_type),
            (axum::http::header::CONTENT_DISPOSITION, disposition),
        ],
        bytes,
    ))
}

/// Re-run phishing analysis (e.g. after a brands-list update). Fetches the
/// raw message via IMAP; analyse_and_store overwrites the previous result.
pub async fn reanalyse_message(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(message_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let row: Option<(String, String, i64)> =
        sqlx::query_as("SELECT account_id, folder_id, uid FROM messages WHERE id = ?")
            .bind(&message_id)
            .fetch_optional(&user_db)
            .await?;
    let (account_id, folder_id, uid) = row.ok_or(AppError::NotFound)?;

    fetch_body_on_demand(
        &state,
        &user.0,
        &user_db,
        &message_id,
        account_id,
        folder_id,
        uid as u32,
    )
    .await
    .map_err(|_| AppError::Internal("reanalysis fetch failed".into()))?;

    let result: Option<(i32, String, String)> = sqlx::query_as(
        "SELECT score, verdict, checks_json FROM phishing_analysis WHERE message_id = ?",
    )
    .bind(&message_id)
    .fetch_optional(&user_db)
    .await?;
    let (score, verdict, checks_json) = result.ok_or(AppError::NotFound)?;

    Ok(Json(json!({
        "score": score,
        "verdict": verdict,
        "checks": serde_json::from_str::<serde_json::Value>(&checks_json).unwrap_or_else(|_| json!([])),
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
    refresh_unread_counts(&user_db).await;

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

    queue_imap_flag(
        &state,
        &user.0,
        &user_db,
        &message_id,
        "flagged",
        req.is_flagged,
    )
    .await;

    Ok(Json(json!({ "ok": true })))
}

pub async fn archive_message(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(message_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let is_local_draft: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM messages WHERE id = ? AND is_local_draft = 1)",
    )
    .bind(&message_id)
    .fetch_one(&user_db)
    .await?;
    if is_local_draft {
        return Err(AppError::Unprocessable(
            "local drafts cannot be archived".into(),
        ));
    }
    let (account_id, uid, folder_full_path) = get_message_location(&user_db, &message_id).await?;

    // Queue IMAP MOVE to Archive
    state
        .sync_manager
        .queue_imap_move(
            user.0.clone(),
            account_id,
            uid,
            folder_full_path,
            "Archive".into(),
            false,
        )
        .await;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_message(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(message_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let is_local_draft: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM messages WHERE id = ? AND is_local_draft = 1)",
    )
    .bind(&message_id)
    .fetch_one(&user_db)
    .await?;
    if is_local_draft {
        crate::routes::drafts::delete_local_draft(&state, &user_db, &message_id).await?;
        return Ok(StatusCode::NO_CONTENT);
    }
    let (account_id, uid, folder_full_path) = get_message_location(&user_db, &message_id).await?;

    // Already-in-trash means hard delete. Detect by the folder's type, not its
    // name — localized servers call it "Papierkorb", "Corbeille", etc.
    let src_type: Option<String> = sqlx::query_scalar(
        "SELECT f.folder_type FROM messages m JOIN folders f ON f.id = m.folder_id WHERE m.id = ?",
    )
    .bind(&message_id)
    .fetch_optional(&user_db)
    .await?
    .flatten();
    let is_trash = src_type.as_deref() == Some("TRASH");

    if is_trash {
        // Hard delete — EXPUNGE
        state
            .sync_manager
            .queue_imap_expunge(user.0.clone(), account_id, uid, folder_full_path)
            .await;
    } else {
        // Soft delete — move to Trash
        state
            .sync_manager
            .queue_imap_move(
                user.0.clone(),
                account_id,
                uid,
                folder_full_path,
                "Trash".into(),
                false,
            )
            .await;
    }

    sqlx::query("UPDATE messages SET is_deleted = 1 WHERE id = ?")
        .bind(&message_id)
        .execute(&user_db)
        .await?;
    refresh_unread_counts(&user_db).await;

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

    // Get destination folder path. The target must belong to the same account
    // because IMAP MOVE cannot cross account boundaries.
    let dest_path: Option<String> =
        sqlx::query_scalar("SELECT full_path FROM folders WHERE id = ? AND account_id = ?")
            .bind(&req.folder_id)
            .bind(&account_id)
            .fetch_optional(&user_db)
            .await?;
    let dest_path = dest_path.ok_or(AppError::NotFound)?;

    state
        .sync_manager
        .queue_imap_move(
            user.0.clone(),
            account_id.clone(),
            uid,
            src_folder,
            dest_path,
            false,
        )
        .await;

    sqlx::query("UPDATE messages SET is_deleted = 1 WHERE id = ?")
        .bind(&message_id)
        .execute(&user_db)
        .await?;
    refresh_unread_counts(&user_db).await;
    let _ = state.sync_manager.force_poll(&account_id).await;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn mark_not_spam(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(message_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let (account_id, uid, src_folder) = get_message_location(&user_db, &message_id).await?;

    trust_sender_of(&user_db, &message_id).await;

    let inbox: Option<(String, String)> = sqlx::query_as(
        "SELECT id, full_path FROM folders WHERE account_id = ? AND folder_type = 'INBOX' ORDER BY full_path LIMIT 1",
    )
    .bind(&account_id)
    .fetch_optional(&user_db)
    .await?;
    let (inbox_id, inbox_path) = inbox.ok_or(AppError::NotFound)?;

    if src_folder.eq_ignore_ascii_case(&inbox_path) {
        sqlx::query("UPDATE messages SET folder_id = ?, is_deleted = 0 WHERE id = ?")
            .bind(&inbox_id)
            .bind(&message_id)
            .execute(&user_db)
            .await?;
        refresh_unread_counts(&user_db).await;
        return Ok(StatusCode::NO_CONTENT);
    }

    // Only mail sitting in the spam folder gets moved. "Not spam" on a
    // flagged message elsewhere (inbox banner, archive) just teaches the
    // filter — yanking it into the inbox would be surprising.
    let src_type: Option<String> =
        sqlx::query_scalar("SELECT folder_type FROM folders WHERE account_id = ? AND full_path = ?")
            .bind(&account_id)
            .bind(&src_folder)
            .fetch_optional(&user_db)
            .await?;
    if src_type.as_deref() != Some("SPAM") {
        return Ok(StatusCode::NO_CONTENT);
    }

    state
        .sync_manager
        .queue_imap_move(
            user.0.clone(),
            account_id.clone(),
            uid,
            src_folder,
            inbox_path,
            false,
        )
        .await;

    sqlx::query("UPDATE messages SET is_deleted = 1 WHERE id = ?")
        .bind(&message_id)
        .execute(&user_db)
        .await?;
    refresh_unread_counts(&user_db).await;
    let _ = state.sync_manager.force_poll(&account_id).await;

    Ok(StatusCode::NO_CONTENT)
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Learn from a "not spam" click: trust the message's sender domain and clear
/// the phishing verdict on every flagged message from that organisation, so
/// the user corrects a false positive once instead of per message. Future
/// analyses short-circuit on the trusted domain. Best-effort — a failure here
/// must not block the actual not-spam move.
async fn trust_sender_of(user_db: &sqlx::SqlitePool, message_id: &str) {
    let from_addr: Option<String> =
        sqlx::query_scalar("SELECT from_addr FROM messages WHERE id = ?")
            .bind(message_id)
            .fetch_optional(user_db)
            .await
            .ok()
            .flatten();
    let Some(domain) = from_addr.as_deref().and_then(phishing::sender_trust_domain) else {
        return;
    };

    let _ = sqlx::query("INSERT OR IGNORE INTO phishing_trusted_senders (domain) VALUES (?)")
        .bind(&domain)
        .execute(user_db)
        .await;

    // Match flagged messages by registrable domain in Rust — SQL LIKE can't
    // express "same organisation" without over-matching lookalike domains.
    let flagged: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, from_addr FROM messages WHERE phishing_verdict IN ('suspicious', 'phishing')",
    )
    .fetch_all(user_db)
    .await
    .unwrap_or_default();
    for (id, addr) in flagged {
        if phishing::sender_trust_domain(&addr).as_deref() != Some(domain.as_str()) {
            continue;
        }
        let _ = sqlx::query("UPDATE messages SET phishing_verdict = 'clean' WHERE id = ?")
            .bind(&id)
            .execute(user_db)
            .await;
        let _ = sqlx::query("UPDATE phishing_analysis SET verdict = 'clean' WHERE message_id = ?")
            .bind(&id)
            .execute(user_db)
            .await;
    }
}

/// Recompute every folder's cached unread_count from the messages table.
/// The cache is otherwise only refreshed by IMAP sync, so any API mutation
/// that touches is_read or is_deleted must call this or the sidebar badges
/// go stale. A full recompute is cheap (few folders, messages.folder_id is
/// indexed) and stays correct for multi-folder mutations like thread actions.
pub(crate) async fn refresh_unread_counts(db: &sqlx::SqlitePool) {
    let _ = sqlx::query(
        "UPDATE folders SET unread_count = (SELECT COUNT(*) FROM messages WHERE folder_id = folders.id AND is_read = 0 AND is_deleted = 0)",
    )
    .execute(db)
    .await;
}

async fn require_message_exists(db: &sqlx::SqlitePool, message_id: &str) -> Result<(), AppError> {
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM messages WHERE id = ?")
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
    _user_id: &str,
    user_db: &sqlx::SqlitePool,
    message_id: &str,
    account_id: String,
    folder_id: String,
    uid: u32,
) -> Result<(Option<String>, Option<String>), AppError> {
    // Get account credentials + backend kind
    let row: Option<(Vec<u8>, String, String, i64, String, Option<String>, String)> = sqlx::query_as(
        "SELECT credentials_encrypted, primary_email, imap_host, imap_port, imap_auth_scheme, imap_tls_cert, provider_kind FROM email_accounts WHERE id = ?",
    )
    .bind(&account_id)
    .fetch_optional(user_db)
    .await?;

    let (creds_enc, primary_email, host, port, auth_scheme, imap_tls_cert, provider_kind) =
        row.ok_or(AppError::NotFound)?;
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

    // Prefer a freshly refreshed OAuth token over the (possibly expired) stored one.
    let oauth_access_token =
        match crate::oauth_tokens::fresh_access_token(&state.credential_key, user_db, &account_id)
            .await
        {
            Ok(Some(token)) => Some(token),
            Ok(None) => creds["oauth_access_token"].as_str().map(|s| s.to_owned()),
            Err(error) if crate::oauth_tokens::is_reauth_required(&error) => {
                return Err(AppError::Unprocessable(
                    "oauth_reauthentication_required".into(),
                ));
            }
            Err(error) => {
                return Err(AppError::BadGateway(format!(
                    "oauth token refresh failed: {error}"
                )));
            }
        };

    let config = mail_sync::provider::ProviderConfig {
        host,
        port: port as u16,
        username: creds["imap_username"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&primary_email)
            .trim()
            .to_ascii_lowercase(),
        password: creds["imap_password"].as_str().unwrap_or("").to_owned(),
        oauth_access_token,
        auth_scheme,
        trusted_cert_der: mail_sync::session::decode_trusted_cert(imap_tls_cert.as_deref()),
        db: Some(user_db.clone()),
        account_id: account_id.clone(),
    };

    let result = mail_sync::fetch_body_by_uid(
        mail_sync::provider::ProviderKind::parse(&provider_kind),
        &config,
        &folder_path,
        uid,
        state.blob_store.clone(),
        &account_id,
        message_id,
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
        enqueue_flag_op(state, user_id, db, &account_id, uid, &folder_path, flag, set).await;
    }
}

/// Record a flag change as pending and nudge the account's sync task to push it.
///
/// The row in `pending_flag_ops` is what makes the change survive: the nudge is
/// best-effort (no sync task, full channel, failed IMAP command), and until the
/// server confirms the flag, the pending row also stops the next flag
/// reconciliation from copying the server's stale value back over the local one.
pub async fn enqueue_flag_op(
    state: &AppState,
    user_id: &str,
    db: &sqlx::SqlitePool,
    account_id: &str,
    uid: i64,
    folder_path: &str,
    flag: &str,
    set: bool,
) {
    let stored = sqlx::query(
        "INSERT INTO pending_flag_ops (account_id, folder_path, uid, flag, value) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(account_id, folder_path, uid, flag) DO UPDATE SET value = excluded.value, attempts = 0",
    )
    .bind(account_id)
    .bind(folder_path)
    .bind(uid)
    .bind(flag)
    .bind(set)
    .execute(db)
    .await;
    if let Err(error) = stored {
        tracing::warn!("pending flag op not stored: message flag may revert: {error}");
    }
    state
        .sync_manager
        .queue_flag_flush(user_id.to_owned(), account_id)
        .await;
}
