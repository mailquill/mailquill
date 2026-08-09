use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    error::AppError, middleware::UserId, routes::messages::refresh_unread_counts, state::AppState,
};

#[derive(Deserialize)]
pub struct ThreadReadRequest {
    is_read: bool,
}

pub async fn get_thread(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(thread_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    let rows: Vec<(
        String, String, String, i64, Option<String>, Option<String>, Option<String>,
        String, String, String, String, String, Option<String>, String, bool, bool,
    )> = sqlx::query_as(
        "SELECT m.id, m.account_id, m.folder_id, m.uid, m.message_id_header, m.in_reply_to, m.list_id, m.subject, m.from_addr, m.to_addrs, m.snippet, m.internal_date, f.folder_type, f.full_path, m.is_read, m.is_flagged FROM messages m INDEXED BY idx_msg_thread_undeleted_date LEFT JOIN folders f ON f.id = m.folder_id WHERE m.thread_id = ? AND m.is_deleted = 0 ORDER BY m.internal_date ASC",
    )
    .bind(&thread_id)
    .fetch_all(&user_db)
    .await?;

    if rows.is_empty() {
        return Err(AppError::NotFound);
    }

    // The same message can sit in several folders (e.g. INBOX + Archive); each
    // is a separate row sharing one Message-ID. Collapse them so the thread
    // shows each message once. Rows without a Message-ID are always kept.
    let mut seen = std::collections::HashSet::new();
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|r| match &r.4 {
            Some(header) => seen.insert(header.clone()),
            None => true,
        })
        .collect();

    let total = rows.len();
    let messages: Vec<_> = rows
        .into_iter()
        .map(
            |(
                id,
                account_id,
                folder_id,
                uid,
                message_id_header,
                in_reply_to,
                list_id,
                subject,
                from_addr,
                to_addrs,
                snippet,
                internal_date,
                folder_type,
                folder_path,
                is_read,
                is_flagged,
            )| {
                json!({
                    "id": id,
                    "account_id": account_id,
                    "folder_id": folder_id,
                    "uid": uid,
                    "message_id_header": message_id_header,
                    "in_reply_to": in_reply_to,
                    "list_id": list_id,
                    "subject": subject,
                    "from_addr": from_addr,
                    "to_addrs": to_addrs,
                    "snippet": snippet,
                    "internal_date": internal_date,
                    "folder_type": folder_type,
                    "folder_path": folder_path,
                    "is_read": is_read,
                    "is_flagged": is_flagged,
                })
            },
        )
        .collect();

    let thread_unread: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE thread_id = ? AND is_read = 0 AND is_deleted = 0",
    )
    .bind(&thread_id)
    .fetch_one(&user_db)
    .await
    .unwrap_or(0);

    let participants: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT from_addr FROM messages WHERE thread_id = ? AND is_deleted = 0 ORDER BY internal_date ASC LIMIT 3",
    )
    .bind(&thread_id)
    .fetch_all(&user_db)
    .await
    .unwrap_or_default();

    Ok(Json(json!({
        "thread_id": thread_id,
        "messages": messages,
        "thread_size": total,
        "thread_unread": thread_unread,
        "thread_participants": participants,
    })))
}

pub async fn archive_thread(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(thread_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    bulk_thread_action(&state, &user.0, &thread_id, ThreadAction::Archive).await
}

pub async fn delete_thread(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(thread_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    bulk_thread_action(&state, &user.0, &thread_id, ThreadAction::Delete).await
}

pub async fn mark_thread_read(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(thread_id): Path<String>,
    Json(req): Json<ThreadReadRequest>,
) -> Result<impl IntoResponse, AppError> {
    bulk_thread_action(
        &state,
        &user.0,
        &thread_id,
        ThreadAction::MarkRead(req.is_read),
    )
    .await
}

enum ThreadAction {
    Archive,
    Delete,
    MarkRead(bool),
}

async fn bulk_thread_action(
    state: &AppState,
    user_id: &str,
    thread_id: &str,
    action: ThreadAction,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(user_id).await?;

    let messages: Vec<(String, String, i64, String, bool, Option<String>)> = sqlx::query_as(
        "SELECT m.id, m.account_id, m.uid, f.full_path, m.is_local_draft, f.folder_type FROM messages m JOIN folders f ON f.id = m.folder_id WHERE m.thread_id = ? AND m.is_deleted = 0",
    )
    .bind(thread_id)
    .fetch_all(&user_db)
    .await?;

    if messages.is_empty() {
        return Err(AppError::NotFound);
    }

    for (msg_id, account_id, uid, folder_path, is_local_draft, folder_type) in &messages {
        if *is_local_draft {
            match action {
                ThreadAction::Delete => {
                    crate::routes::drafts::delete_local_draft(state, &user_db, msg_id).await?;
                }
                ThreadAction::MarkRead(is_read) => {
                    sqlx::query("UPDATE messages SET is_read = ? WHERE id = ?")
                        .bind(is_read)
                        .bind(msg_id)
                        .execute(&user_db)
                        .await?;
                }
                ThreadAction::Archive => {}
            }
            continue;
        }
        match action {
            ThreadAction::Archive => {
                state
                    .sync_manager
                    .queue_imap_move(
                        user_id.to_owned(),
                        account_id.clone(),
                        *uid as u32,
                        folder_path.clone(),
                        "Archive".into(),
                        false,
                    )
                    .await;
            }
            ThreadAction::Delete => {
                sqlx::query("UPDATE messages SET is_deleted = 1 WHERE id = ?")
                    .bind(msg_id)
                    .execute(&user_db)
                    .await?;
                // Already-in-trash means hard delete. Detect by the folder's
                // type, not its name — localized servers call it
                // "Papierkorb", "Corbeille", etc. Otherwise a MOVE into the
                // message's own folder either no-ops (message stays "not
                // deleted") or falls back to COPY+flag, leaving a live
                // duplicate the next sync re-imports as new.
                if folder_type.as_deref() == Some("TRASH") {
                    state
                        .sync_manager
                        .queue_imap_expunge(
                            user_id.to_owned(),
                            account_id.clone(),
                            *uid as u32,
                            folder_path.clone(),
                        )
                        .await;
                } else {
                    state
                        .sync_manager
                        .queue_imap_move(
                            user_id.to_owned(),
                            account_id.clone(),
                            *uid as u32,
                            folder_path.clone(),
                            "Trash".into(),
                            false,
                        )
                        .await;
                }
            }
            ThreadAction::MarkRead(is_read) => {
                sqlx::query("UPDATE messages SET is_read = ? WHERE id = ?")
                    .bind(is_read)
                    .bind(msg_id)
                    .execute(&user_db)
                    .await?;
                state
                    .sync_manager
                    .queue_imap_flag(
                        user_id.to_owned(),
                        account_id.clone(),
                        *uid as u32,
                        folder_path.clone(),
                        "seen".into(),
                        is_read,
                    )
                    .await;
            }
        }
    }

    if matches!(action, ThreadAction::Delete | ThreadAction::MarkRead(_)) {
        refresh_unread_counts(&user_db).await;
    }

    Ok(StatusCode::NO_CONTENT)
}
