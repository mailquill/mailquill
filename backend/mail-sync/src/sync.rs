use std::{collections::HashMap, sync::Arc};
use tokio::{sync::mpsc, time};
use tracing::{error, info, warn};

use crate::{
    manager::{NewMessageNotification, SyncAppState, SyncCommand},
    mime::{compute_snippet, parse_mime},
    provider::{self, MailProvider, ProviderConfig, ProviderKind},
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

    let interval_secs: i64 =
        sqlx::query_scalar("SELECT sync_interval_secs FROM email_accounts WHERE id = ?")
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
    // Reset progress counters for this run, then mark syncing.
    app.sync_manager().set_progress(account_id, 0, 0).await;
    app.sync_manager()
        .set_state(account_id, "syncing", None, None)
        .await;

    let result = sync_account(account_id, user_id, app).await;

    match result {
        Ok(()) => {
            app.sync_manager()
                .set_state(
                    account_id,
                    "idle",
                    Some(chrono::Utc::now().to_rfc3339()),
                    None,
                )
                .await;
        }
        Err(e) => {
            error!("sync error for account={account_id}: {e}");
            app.sync_manager()
                .set_state(account_id, "error", None, Some(e.to_string()))
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

    let body_sync_mode: String =
        sqlx::query_scalar("SELECT body_sync_mode FROM email_accounts WHERE id = ?")
            .bind(account_id)
            .fetch_optional(&db)
            .await?
            .ok_or("account not found")?;

    let mut provider = open_provider(account_id, user_id, app).await?;

    // Discover folders (task 4.1)
    let folders = provider.list_folders().await?;
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

    // Progress total: sum of server-reported message counts across all folders.
    // A cheap status pre-pass gives a stable denominator before the backfill
    // starts inserting rows.
    let mut total: i64 = 0;
    for folder in &folders {
        if let Ok(status) = provider.folder_status(&folder.full_path).await {
            total += status.exists as i64;
        }
    }
    let already: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE account_id = ?")
        .bind(account_id)
        .fetch_one(&db)
        .await
        .unwrap_or(0);
    app.sync_manager()
        .set_progress(account_id, already, total)
        .await;

    // Sync each folder
    for folder in &folders {
        if let Err(e) = sync_folder(
            account_id,
            user_id,
            &folder.full_path,
            &body_sync_mode,
            total,
            provider.as_mut(),
            &db,
            app,
        )
        .await
        {
            warn!("folder sync error: folder={} err={e}", folder.full_path);
        }
    }

    let _ = provider.close().await;
    Ok(())
}

/// Open the account's mailbox backend (IMAP or one of the API providers,
/// chosen by `email_accounts.provider_kind`).
async fn open_provider(
    account_id: &str,
    user_id: &str,
    app: &Arc<dyn SyncAppState>,
) -> Result<Box<dyn MailProvider>, Box<dyn std::error::Error + Send + Sync>> {
    let db = app.user_db(user_id).await?;
    let row: Option<(Vec<u8>, String, i64, String, Option<String>, String)> = sqlx::query_as(
        "SELECT credentials_encrypted, imap_host, imap_port, imap_auth_scheme, imap_tls_cert, provider_kind FROM email_accounts WHERE id = ?",
    )
    .bind(account_id)
    .fetch_optional(&db)
    .await?;

    let (creds_enc, host, port, auth_scheme, tls_cert, kind) = row.ok_or("account not found")?;
    let creds_bytes = app.credential_key().decrypt(&creds_enc)?;
    let creds: serde_json::Value = serde_json::from_slice(&creds_bytes)?;

    // Prefer a freshly refreshed OAuth token over the stored one (the stored
    // access token may be expired; the api layer refreshes and persists it).
    let oauth_access_token = match app.fresh_oauth_token(user_id, account_id).await {
        Some(token) => Some(token),
        None => creds["oauth_access_token"].as_str().map(|s| s.to_owned()),
    };

    let config = ProviderConfig {
        host,
        port: port as u16,
        username: creds["imap_username"].as_str().unwrap_or("").to_owned(),
        password: creds["imap_password"].as_str().unwrap_or("").to_owned(),
        oauth_access_token,
        auth_scheme,
        trusted_cert_der: crate::session::decode_trusted_cert(tls_cert.as_deref()),
        db: Some(db),
        account_id: account_id.to_owned(),
    };
    provider::connect(ProviderKind::parse(&kind), &config)
        .await
        .map_err(|e| Box::new(e) as _)
}

#[allow(clippy::too_many_arguments)]
async fn sync_folder(
    account_id: &str,
    user_id: &str,
    folder_path: &str,
    body_sync_mode: &str,
    total: i64,
    provider: &mut dyn MailProvider,
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

    // Folder status (IMAP: SELECT; APIs: metadata lookup)
    let server_uidvalidity = provider.folder_status(folder_path).await?.uidvalidity;

    // Handle UIDVALIDITY change — purge and full re-sync (task 4.5)
    if let Some(sv) = stored_uidvalidity {
        if sv != server_uidvalidity as i64 {
            warn!("UIDVALIDITY changed for folder={folder_path}, purging and re-syncing");
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

    // Incremental sync in bounded UID chunks (task 4.5). Fetching the whole
    // `uid_start:*` range at once buffers every message before a single row is
    // written — on large mailboxes (tens of thousands of messages) that stalls
    // or times out, so nothing is committed. Chunking keeps each fetch small,
    // commits progress per chunk, and lets messages stream into the UI.
    const UID_CHUNK: u32 = 500;
    // IMAP UIDs are u32; last_uid is stored as i64. Work in u32 for the walk.
    let prev_last_uid: u32 = last_uid.unwrap_or(0).clamp(0, u32::MAX as i64) as u32;
    let uid_start: u32 = prev_last_uid + 1;

    // Reconcile read/flagged state of already-synced messages with the server.
    // Header backfill below only fetches NEW uids, so server-side flag changes on
    // existing messages (e.g. read on another device) would otherwise never sync.
    if prev_last_uid > 0 {
        reconcile_flags(&folder_id, folder_path, prev_last_uid, provider, db).await?;
    }

    // Highest UID currently in the mailbox bounds the chunk walk.
    let highest = match provider.highest_uid(folder_path).await? {
        Some(h) if h >= uid_start => h,
        _ => {
            // No new messages; flags already reconciled — refresh unread and finish.
            update_unread_count(&folder_id, db).await?;
            return Ok(());
        }
    };

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
    let account_name =
        sqlx::query_scalar::<_, String>("SELECT display_name FROM email_accounts WHERE id = ?")
            .bind(account_id)
            .fetch_optional(db)
            .await?
            .unwrap_or_else(|| "Mail account".to_owned());

    let mut chunk_start = uid_start;
    while chunk_start <= highest {
        let chunk_end = chunk_start.saturating_add(UID_CHUNK - 1).min(highest);
        let uid_set = format!("{}:{}", chunk_start, chunk_end);

        let messages = if body_sync_mode == "full" {
            // Full sync: fetch complete raw messages (task 4.3)
            provider.fetch_full(folder_path, &uid_set).await?
        } else {
            // Lazy sync: fetch headers only (task 4.2)
            provider.fetch_headers(folder_path, &uid_set).await?
        };

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
            "INSERT INTO messages (account_id, folder_id, uid, message_id_header, thread_id, in_reply_to, \"references\", list_id, subject, subject_normalized, snippet, from_addr, to_addrs, cc_addrs, date, internal_date, is_read, is_flagged) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(folder_id, uid) DO UPDATE SET thread_id = excluded.thread_id, is_read = excluded.is_read, is_flagged = excluded.is_flagged, subject = excluded.subject, subject_normalized = excluded.subject_normalized, snippet = excluded.snippet, from_addr = excluded.from_addr, to_addrs = excluded.to_addrs, cc_addrs = excluded.cc_addrs RETURNING id",
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
                            "INSERT OR IGNORE INTO attachments (message_id, filename, content_type, content_id, size_bytes, blob_key) VALUES (?, ?, ?, ?, ?, ?)",
                        )
                        .bind(&msg_db_id)
                        .bind(att.filename.as_deref())
                        .bind(&att.content_type)
                        .bind(att.content_id.as_deref())
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

                }
            }

            // Phishing analysis on every new message (full sync: complete raw
            // message; lazy sync: header block — body checks don't fire).
            if is_new_message {
                if let Some(ref raw) = msg.body {
                    phishing::analyse_and_store(db, &msg_db_id, raw).await;
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

        // Persist progress after each chunk (task 4.14) so messages stream into
        // the UI and an interrupted sync resumes from the last committed UID.
        let unread_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE folder_id = ? AND is_read = 0 AND is_deleted = 0",
        )
        .bind(&folder_id)
        .fetch_one(db)
        .await
        .unwrap_or(0);

        sqlx::query("UPDATE folders SET last_uid = ?, unread_count = ? WHERE id = ?")
            .bind(max_uid)
            .bind(unread_count)
            .bind(&folder_id)
            .execute(db)
            .await?;

        // Report sync progress (synced / total) for the live UI indicator.
        let synced: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE account_id = ?")
            .bind(account_id)
            .fetch_one(db)
            .await
            .unwrap_or(0);
        app.sync_manager()
            .set_progress(account_id, synced, total)
            .await;

        if chunk_end >= highest {
            break;
        }
        chunk_start = chunk_end + 1;
    }

    Ok(())
}

/// Reconcile read/flagged state of stored messages (UIDs `1..=up_to_uid`) with
/// the server by fetching FLAGS in chunks and updating rows that exist locally.
async fn reconcile_flags(
    folder_id: &str,
    folder_path: &str,
    up_to_uid: u32,
    provider: &mut dyn MailProvider,
    db: &sqlx::SqlitePool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    const FLAG_CHUNK: u32 = 1000;
    let mut start: u32 = 1;
    while start <= up_to_uid {
        let end = start.saturating_add(FLAG_CHUNK - 1).min(up_to_uid);
        let flags = provider
            .fetch_flags(folder_path, &format!("{}:{}", start, end))
            .await?;
        for (uid, seen, flagged) in flags {
            sqlx::query(
                "UPDATE messages SET is_read = ?, is_flagged = ? WHERE folder_id = ? AND uid = ?",
            )
            .bind(seen as i64)
            .bind(flagged as i64)
            .bind(folder_id)
            .bind(uid as i64)
            .execute(db)
            .await?;
        }
        if end >= up_to_uid {
            break;
        }
        start = end + 1;
    }
    Ok(())
}

/// Recompute and persist a folder's unread count.
async fn update_unread_count(
    folder_id: &str,
    db: &sqlx::SqlitePool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let unread: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE folder_id = ? AND is_read = 0 AND is_deleted = 0",
    )
    .bind(folder_id)
    .fetch_one(db)
    .await
    .unwrap_or(0);
    sqlx::query("UPDATE folders SET unread_count = ? WHERE id = ?")
        .bind(unread)
        .bind(folder_id)
        .execute(db)
        .await?;
    Ok(())
}

// ── queued flag/move operations (provider-agnostic) ──────────────────────────

async fn do_imap_move(
    account_id: &str,
    user_id: &str,
    uid: u32,
    src_folder: &str,
    dest_folder: &str,
    _expunge: bool,
    app: &Arc<dyn SyncAppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut provider = open_provider(account_id, user_id, app).await?;
    provider.move_message(src_folder, uid, dest_folder).await?;
    let _ = provider.close().await;
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
    let mut provider = open_provider(account_id, user_id, app).await?;
    provider.set_flag(folder, uid, flag, set).await?;
    let _ = provider.close().await;
    Ok(())
}

async fn do_imap_expunge(
    account_id: &str,
    user_id: &str,
    uid: u32,
    folder: &str,
    app: &Arc<dyn SyncAppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut provider = open_provider(account_id, user_id, app).await?;
    provider.delete_permanently(folder, uid).await?;
    let _ = provider.close().await;
    Ok(())
}
