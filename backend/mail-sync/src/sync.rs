use std::{collections::HashMap, sync::Arc};
use tokio::{sync::mpsc, time};
use tracing::{error, info, warn};

use crate::{
    manager::{NewMessageNotification, SyncAppState, SyncCommand, SyncStatus},
    mime::{compute_snippet, parse_mime},
    threading::assign_thread_id,
};

/// Main sync task loop per account (tasks 4.12-4.13).
pub async fn run_sync_task(
    account_id: String,
    user_id: String,
    mut rx: mpsc::Receiver<SyncCommand>,
    app_state: Arc<dyn SyncAppState>,
) {
    info!("sync task starting: account={account_id}");

    // Initial sync
    do_sync(&account_id, &user_id, &app_state).await;

    let db = match app_state.user_db(&user_id).await {
        Ok(db) => db,
        Err(e) => {
            error!("sync task: cannot open user db: {e}");
            return;
        }
    };

    let interval_secs: i64 = sqlx::query_scalar(
        "SELECT sync_interval_secs FROM email_accounts WHERE id = ?",
    )
    .bind(&account_id)
    .fetch_optional(&db)
    .await
    .unwrap_or(None)
    .unwrap_or(300);

    let mut ticker = time::interval(time::Duration::from_secs(interval_secs as u64));
    ticker.reset();

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                do_sync(&account_id, &user_id, &app_state).await;
            }
            cmd = rx.recv() => {
                match cmd {
                    Some(SyncCommand::Shutdown) | None => {
                        info!("sync task stopping: account={account_id}");
                        break;
                    }
                    Some(SyncCommand::ForcePoll) => {
                        do_sync(&account_id, &user_id, &app_state).await;
                    }
                    Some(SyncCommand::IMapMove { user_id: uid, uid: msg_uid, src_folder, dest_folder, expunge }) => {
                        if let Err(e) = do_imap_move(&account_id, &uid, msg_uid, &src_folder, &dest_folder, expunge, &app_state).await {
                            warn!("IMAP move failed: {e}");
                        }
                    }
                    Some(SyncCommand::IMapFlag { user_id: uid, uid: msg_uid, folder, flag, set }) => {
                        if let Err(e) = do_imap_flag(&account_id, &uid, msg_uid, &folder, &flag, set, &app_state).await {
                            warn!("IMAP flag failed: {e}");
                        }
                    }
                    Some(SyncCommand::IMapExpunge { user_id: uid, uid: msg_uid, folder }) => {
                        if let Err(e) = do_imap_expunge(&account_id, &uid, msg_uid, &folder, &app_state).await {
                            warn!("IMAP expunge failed: {e}");
                        }
                    }
                }
            }
        }
    }
}

async fn do_sync(account_id: &str, user_id: &str, app: &Arc<dyn SyncAppState>) {
    app.sync_manager()
        .update_status(
            account_id,
            SyncStatus {
                state: "syncing".into(),
                last_synced_at: None,
                error: None,
            },
        )
        .await;

    let result = sync_account(account_id, user_id, app).await;

    match result {
        Ok(()) => {
            app.sync_manager()
                .update_status(
                    account_id,
                    SyncStatus {
                        state: "idle".into(),
                        last_synced_at: Some(chrono::Utc::now().to_rfc3339()),
                        error: None,
                    },
                )
                .await;
        }
        Err(e) => {
            error!("sync error for account={account_id}: {e}");
            app.sync_manager()
                .update_status(
                    account_id,
                    SyncStatus {
                        state: "error".into(),
                        last_synced_at: None,
                        error: Some(e.to_string()),
                    },
                )
                .await;
        }
    }
}

async fn sync_account(
    account_id: &str,
    user_id: &str,
    app: &Arc<dyn SyncAppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let db = app.user_db(user_id).await?;

    let row: Option<(Vec<u8>, String, i64, String, String)> = sqlx::query_as(
        "SELECT credentials_encrypted, imap_host, imap_port, imap_auth_scheme, body_sync_mode FROM email_accounts WHERE id = ?",
    )
    .bind(account_id)
    .fetch_optional(&db)
    .await?;

    let (creds_enc, host, port, auth_scheme, body_sync_mode) = row.ok_or("account not found")?;
    let creds_bytes = app.credential_key().decrypt(&creds_enc)?;
    let creds: serde_json::Value = serde_json::from_slice(&creds_bytes)?;

    let imap_user = creds["imap_username"].as_str().unwrap_or("").to_owned();
    let imap_pass = creds["imap_password"].as_str().unwrap_or("").to_owned();
    let oauth_token = creds["oauth_access_token"].as_str().map(|s| s.to_owned());

    // Open IMAP connection
    let mut session = crate::session::connect_imap(
        &host,
        port as u16,
        &imap_user,
        &imap_pass,
        oauth_token.as_deref(),
        &auth_scheme,
    )
    .await?;

    // Discover folders (task 4.1)
    let folders = crate::session::list_folders(&mut session).await?;
    for folder in &folders {
        sqlx::query(
            "INSERT INTO folders (account_id, name, full_path, folder_type) VALUES (?, ?, ?, ?) ON CONFLICT(account_id, full_path) DO UPDATE SET name = excluded.name, folder_type = excluded.folder_type",
        )
        .bind(account_id)
        .bind(&folder.name)
        .bind(&folder.full_path)
        .bind(&folder.folder_type)
        .execute(&db)
        .await?;
    }

    // Check THREAD=REFERENCES capability (task 5.1)
    let has_thread_cmd = crate::session::check_thread_capability(&mut session).await;

    // Sync each folder
    for folder in &folders {
        if let Err(e) = sync_folder(
            account_id,
            user_id,
            &folder.full_path,
            has_thread_cmd,
            &body_sync_mode,
            &mut session,
            &db,
            app,
        )
        .await
        {
            warn!("folder sync error: folder={} err={e}", folder.full_path);
        }
    }

    Ok(())
}

async fn sync_folder(
    account_id: &str,
    user_id: &str,
    folder_path: &str,
    _has_thread_cmd: bool,
    body_sync_mode: &str,
    session: &mut crate::session::ImapSession,
    db: &sqlx::SqlitePool,
    app: &Arc<dyn SyncAppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Get stored folder info
    let folder_row: Option<(String, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT id, uidvalidity, last_uid FROM folders WHERE account_id = ? AND full_path = ?",
    )
    .bind(account_id)
    .bind(folder_path)
    .fetch_optional(db)
    .await?;

    let (folder_id, stored_uidvalidity, last_uid) = folder_row.ok_or("folder not found")?;

    // Select folder
    let (server_uidvalidity, _exists) =
        crate::session::select_folder(session, folder_path).await?;

    // Handle UIDVALIDITY change — purge and full re-sync (task 4.5)
    if let Some(sv) = stored_uidvalidity {
        if sv != server_uidvalidity as i64 {
            warn!(
                "UIDVALIDITY changed for folder={folder_path}, purging and re-syncing"
            );
            sqlx::query("DELETE FROM messages WHERE folder_id = ?")
                .bind(&folder_id)
                .execute(db)
                .await?;
            sqlx::query("UPDATE folders SET uidvalidity = ?, last_uid = NULL WHERE id = ?")
                .bind(server_uidvalidity as i64)
                .bind(&folder_id)
                .execute(db)
                .await?;
        }
    } else {
        sqlx::query("UPDATE folders SET uidvalidity = ? WHERE id = ?")
            .bind(server_uidvalidity as i64)
            .bind(&folder_id)
            .execute(db)
            .await?;
    }

    // Incremental sync: UIDs > last_uid (task 4.5)
    let uid_start = last_uid.unwrap_or(0) + 1;
    let uid_set = format!("{}:*", uid_start);

    let messages = if body_sync_mode == "full" {
        // Full sync: fetch RFC822 (task 4.3)
        crate::session::fetch_full(session, &uid_set).await?
    } else {
        // Lazy sync: fetch headers only (task 4.2)
        crate::session::fetch_headers(session, &uid_set).await?
    };

    if messages.is_empty() {
        return Ok(());
    }

    // Build existing thread_id map for JWZ (task 5.2)
    let existing_threads: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT message_id_header, thread_id FROM messages WHERE account_id = ? AND message_id_header IS NOT NULL AND thread_id IS NOT NULL",
    )
    .bind(account_id)
    .fetch_all(db)
    .await
    .unwrap_or_default()
    .into_iter()
    .filter_map(|(mid, tid)| if tid.is_empty() { None } else { Some((mid, tid)) })
    .collect();

    let mut max_uid = last_uid.unwrap_or(0);
    let account_name = sqlx::query_scalar::<_, String>(
        "SELECT display_name FROM email_accounts WHERE id = ?",
    )
    .bind(account_id)
    .fetch_optional(db)
    .await?
    .unwrap_or_else(|| "Mail account".to_owned());

    for msg in &messages {
        if msg.uid > max_uid as u32 {
            max_uid = msg.uid as i64;
        }

        let thread_id = assign_thread_id(
            msg.message_id.as_deref(),
            msg.in_reply_to.as_deref(),
            msg.references.as_deref(),
            msg.list_id.as_deref(),
            &msg.subject,
            &existing_threads,
        );

        let subject_normalized = crate::threading::normalize_subject(&msg.subject);

        let snippet = if let Some(ref body) = msg.body {
            if let Ok(parsed) = parse_mime(body) {
                compute_snippet(&msg.subject, parsed.text.as_deref())
            } else {
                compute_snippet(&msg.subject, None)
            }
        } else {
            compute_snippet(&msg.subject, None)
        };

        let is_new_message: bool = !sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM messages WHERE folder_id = ? AND uid = ?)",
        )
        .bind(&folder_id)
        .bind(msg.uid as i64)
        .fetch_one(db)
        .await?;

        // Insert or update message (task 4.6)
        let msg_id: Option<String> = sqlx::query_scalar(
            "INSERT INTO messages (account_id, folder_id, uid, message_id_header, thread_id, in_reply_to, references, list_id, subject, subject_normalized, snippet, from_addr, to_addrs, cc_addrs, date, internal_date, is_read, is_flagged) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(folder_id, uid) DO UPDATE SET thread_id = excluded.thread_id, is_read = excluded.is_read, is_flagged = excluded.is_flagged RETURNING id",
        )
        .bind(account_id)
        .bind(&folder_id)
        .bind(msg.uid as i64)
        .bind(msg.message_id.as_deref())
        .bind(&thread_id)
        .bind(msg.in_reply_to.as_deref())
        .bind(msg.references.as_deref())
        .bind(msg.list_id.as_deref())
        .bind(&msg.subject)
        .bind(&subject_normalized)
        .bind(&snippet)
        .bind(&msg.from_addr)
        .bind(&msg.to_addrs)
        .bind(&msg.cc_addrs)
        .bind(msg.date.as_deref())
        .bind(&msg.internal_date)
        .bind(msg.is_seen as i64)
        .bind(msg.is_flagged as i64)
        .fetch_optional(db)
        .await?
        .flatten();

        let msg_db_id = match msg_id {
            Some(id) => id,
            None => continue,
        };

        // For full sync mode, store body in blob store (task 4.3)
        if body_sync_mode == "full" {
            if let Some(ref raw_body) = msg.body {
                let parsed = parse_mime(raw_body).unwrap_or_default();

                // Compress and store body
                let body_json = serde_json::json!({
                    "html": parsed.html,
                    "text": parsed.text,
                });
                let body_bytes = serde_json::to_vec(&body_json).unwrap();
                let compressed = mailquill_core::compression::compress_body(&body_bytes);
                let blob = bytes::Bytes::from(compressed.clone());

                let internal_date = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
                let blob_key = mailquill_core::blob::blob_key_body(
                    account_id,
                    &folder_id,
                    msg.uid,
                    internal_date,
                );

                if let Ok(()) = app.blob_store().put(&blob_key, blob).await {
                    let _ = sqlx::query(
                        "INSERT OR IGNORE INTO message_bodies (message_id, blob_key, size_bytes, size_bytes_uncompressed) VALUES (?, ?, ?, ?)",
                    )
                    .bind(&msg_db_id)
                    .bind(&blob_key)
                    .bind(compressed.len() as i64)
                    .bind(body_bytes.len() as i64)
                    .execute(db)
                    .await;
                }

                // Store attachments (task 4.8)
                for (i, att) in parsed.attachments.iter().enumerate() {
                    let att_key = mailquill_core::blob::blob_key_attachment(
                        account_id,
                        &folder_id,
                        msg.uid,
                        internal_date,
                        i,
                    );
                    let att_blob = bytes::Bytes::from(att.data.clone());
                    if let Ok(()) = app.blob_store().put(&att_key, att_blob).await {
                        let _ = sqlx::query(
                            "INSERT OR IGNORE INTO attachments (message_id, filename, content_type, size_bytes, blob_key) VALUES (?, ?, ?, ?, ?)",
                        )
                        .bind(&msg_db_id)
                        .bind(att.filename.as_deref())
                        .bind(&att.content_type)
                        .bind(att.data.len() as i64)
                        .bind(&att_key)
                        .execute(db)
                        .await;
                    }
                }

                // FTS index (task 4.9)
                let body_text = parsed.text.as_deref().unwrap_or("");
                let _ = sqlx::query(
                    "INSERT INTO messages_fts(rowid, subject, from_addr, body_text) VALUES ((SELECT rowid FROM messages WHERE id = ?), ?, ?, ?) ON CONFLICT DO UPDATE SET body_text = excluded.body_text",
                )
                .bind(&msg_db_id)
                .bind(&msg.subject)
                .bind(&msg.from_addr)
                .bind(body_text)
                .execute(db)
                .await;

                // Detect calendar parts (task 4.15)
                if parsed.has_calendar {
                    let _ = sqlx::query(
                        "UPDATE messages SET phishing_verdict = 'has_calendar' WHERE id = ?",
                    )
                    .bind(&msg_db_id)
                    .execute(db)
                    .await;
                }
            }
        }

        // Update FTS for lazy sync with subject + from_addr (no body text)
        if body_sync_mode == "lazy" {
            let _ = sqlx::query(
                "INSERT INTO messages_fts(rowid, subject, from_addr, body_text) VALUES ((SELECT rowid FROM messages WHERE id = ?), ?, ?, '') ON CONFLICT DO NOTHING",
            )
            .bind(&msg_db_id)
            .bind(&msg.subject)
            .bind(&msg.from_addr)
            .execute(db)
            .await;
        }

        if is_new_message {
            let notification = NewMessageNotification {
                message_id: msg_db_id,
                account_id: account_id.to_owned(),
                account_name: account_name.clone(),
                sender: msg.from_addr.clone(),
                subject: msg.subject.clone(),
            };
            if let Err(e) = app.notify_new_message(user_id, notification).await {
                warn!("web push notification failed: {e}");
            }
        }
    }

    // Update last_uid and unread_count (task 4.14)
    let unread_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE folder_id = ? AND is_read = 0 AND is_deleted = 0",
    )
    .bind(&folder_id)
    .fetch_one(db)
    .await
    .unwrap_or(0);

    sqlx::query(
        "UPDATE folders SET last_uid = ?, unread_count = ? WHERE id = ?",
    )
    .bind(max_uid)
    .bind(unread_count)
    .bind(&folder_id)
    .execute(db)
    .await?;

    Ok(())
}

// ── IMAP flag/move operations ─────────────────────────────────────────────────

async fn do_imap_move(
    account_id: &str,
    user_id: &str,
    uid: u32,
    src_folder: &str,
    dest_folder: &str,
    expunge: bool,
    app: &Arc<dyn SyncAppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut session = open_session(account_id, user_id, app).await?;
    crate::session::select_folder(&mut session, src_folder).await?;
    crate::session::move_message(&mut session, uid, dest_folder, expunge).await?;
    Ok(())
}

async fn do_imap_flag(
    account_id: &str,
    user_id: &str,
    uid: u32,
    folder: &str,
    flag: &str,
    set: bool,
    app: &Arc<dyn SyncAppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut session = open_session(account_id, user_id, app).await?;
    crate::session::select_folder(&mut session, folder).await?;
    crate::session::set_flag(&mut session, uid, flag, set).await?;
    Ok(())
}

async fn do_imap_expunge(
    account_id: &str,
    user_id: &str,
    uid: u32,
    folder: &str,
    app: &Arc<dyn SyncAppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut session = open_session(account_id, user_id, app).await?;
    crate::session::select_folder(&mut session, folder).await?;
    crate::session::expunge_uid(&mut session, uid).await?;
    Ok(())
}

async fn open_session(
    account_id: &str,
    user_id: &str,
    app: &Arc<dyn SyncAppState>,
) -> Result<crate::session::ImapSession, Box<dyn std::error::Error + Send + Sync>> {
    let db = app.user_db(user_id).await?;
    let row: Option<(Vec<u8>, String, i64, String)> = sqlx::query_as(
        "SELECT credentials_encrypted, imap_host, imap_port, imap_auth_scheme FROM email_accounts WHERE id = ?",
    )
    .bind(account_id)
    .fetch_optional(&db)
    .await?;

    let (creds_enc, host, port, auth_scheme) = row.ok_or("account not found")?;
    let creds_bytes = app.credential_key().decrypt(&creds_enc)?;
    let creds: serde_json::Value = serde_json::from_slice(&creds_bytes)?;

    let imap_user = creds["imap_username"].as_str().unwrap_or("").to_owned();
    let imap_pass = creds["imap_password"].as_str().unwrap_or("").to_owned();
    let oauth_token = creds["oauth_access_token"].as_str().map(|s| s.to_owned());

    crate::session::connect_imap(
        &host,
        port as u16,
        &imap_user,
        &imap_pass,
        oauth_token.as_deref(),
        &auth_scheme,
    )
    .await
    .map_err(|e| Box::new(e) as _)
}
