use std::{collections::HashMap, sync::Arc};
use tokio::{sync::mpsc, time};
use tracing::{error, info, warn};

use crate::{
    manager::{NewMessageNotification, SyncAppState, SyncCommand},
    mime::{compute_snippet, parse_mime},
    provider::{self, MailProvider, ProviderConfig, ProviderError, ProviderKind},
    threading::assign_thread_id,
};

/// Main sync task loop per account (tasks 4.12-4.13).
pub async fn run_sync_task(
    account_id: String,
    user_id: String,
    mut rx: mpsc::Receiver<SyncCommand>,
    self_tx: mpsc::Sender<SyncCommand>,
    app_state: Arc<dyn SyncAppState>,
) {
    info!("sync task starting: account={account_id}");

    // Initial sync
    do_sync(&account_id, &user_id, &app_state).await;

    let db = match app_state.user_db(&user_id).await {
        Ok(db) => db,
        Err(e) => {
            error!("sync task: account={account_id} user={user_id} cannot open user db: {e}");
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

    // Sync strategy (sync_mode): in 'idle' mode a dedicated IMAP IDLE connection
    // triggers a ForcePoll the moment the server reports new mail (near-instant),
    // and the periodic ticker is disabled. In 'interval' mode there is no IDLE
    // and the ticker polls every sync_interval_secs. IDLE is IMAP-only — Gmail/
    // Outlook API accounts always use the interval path regardless of sync_mode.
    let (provider_kind, sync_mode): (String, String) =
        sqlx::query_as("SELECT provider_kind, sync_mode FROM email_accounts WHERE id = ?")
            .bind(&account_id)
            .fetch_optional(&db)
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| ("imap".into(), "idle".into()));

    let idle_mode = sync_mode == "idle" && ProviderKind::parse(&provider_kind).syncs_over_imap();
    // The guard aborts the IDLE child when this sync task shuts down or is dropped.
    let _idle_guard = if idle_mode {
        let handle = tokio::spawn(run_idle_task(
            account_id.clone(),
            user_id.clone(),
            app_state.clone(),
            self_tx.clone(),
        ));
        Some(AbortOnDrop(handle))
    } else {
        None
    };

    loop {
        tokio::select! {
            // Disabled in idle mode — IDLE drives polling there.
            _ = ticker.tick(), if !idle_mode => {
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
                            warn!("IMAP move failed: account={account_id} user={uid} uid={msg_uid} folder={src_folder} dest={dest_folder} expunge={expunge} err={e}");
                        }
                    }
                    Some(SyncCommand::IMapFlag { user_id: uid, uid: msg_uid, folder, flag, set }) => {
                        if let Err(e) = do_imap_flag(&account_id, &uid, msg_uid, &folder, &flag, set, &app_state).await {
                            warn!("IMAP flag failed: account={account_id} user={uid} uid={msg_uid} folder={folder} flag={flag} set={set} err={e}");
                        }
                    }
                    Some(SyncCommand::IMapExpunge { user_id: uid, uid: msg_uid, folder }) => {
                        if let Err(e) = do_imap_expunge(&account_id, &uid, msg_uid, &folder, &app_state).await {
                            warn!("IMAP expunge failed: account={account_id} user={uid} uid={msg_uid} folder={folder} err={e}");
                        }
                    }
                }
            }
        }
    }
}

async fn do_sync(account_id: &str, user_id: &str, app: &Arc<dyn SyncAppState>) {
    if app.sync_manager().account_status(account_id).await.state == "reauth_required" {
        return;
    }

    // Mark syncing but keep the last known progress counters — sync_account
    // recomputes them as soon as it has the new totals. Resetting to 0/0 here
    // would make a manual refresh flash "0 / 0" until that recompute lands.
    app.sync_manager()
        .set_state(account_id, "syncing", None, None)
        .await;
    publish_sync_status(account_id, user_id, app).await;

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
            let error = e.to_string();
            if error.starts_with("oauth_reauthentication_required:") {
                warn!("sync paused for account={account_id}: OAuth reauthentication required");
                app.sync_manager()
                    .set_state(account_id, "reauth_required", None, Some(error))
                    .await;
            } else {
                error!("sync error for account={account_id}: {error}");
                app.sync_manager()
                    .set_state(account_id, "error", None, Some(error))
                    .await;
            }
        }
    }
    publish_sync_status(account_id, user_id, app).await;
}

async fn publish_sync_status(account_id: &str, user_id: &str, app: &Arc<dyn SyncAppState>) {
    let status = app.sync_manager().account_status(account_id).await;
    let notification = crate::manager::SyncStatusNotification {
        account_id: account_id.to_owned(),
        state: status.state,
        last_synced_at: status.last_synced_at,
        error: status.error,
        synced: status.synced,
        total: status.total,
    };
    if let Err(error) = app.notify_sync_status(user_id, notification).await {
        warn!("sync status SSE failed: account={account_id} error={error}");
    }
}

async fn sync_account(
    account_id: &str,
    user_id: &str,
    app: &Arc<dyn SyncAppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let db = app.user_db(user_id).await?;

    let (body_sync_mode, provider_kind): (String, String) =
        sqlx::query_as("SELECT body_sync_mode, provider_kind FROM email_accounts WHERE id = ?")
            .bind(account_id)
            .fetch_optional(&db)
            .await?
            .ok_or("account not found")?;
    let provider_kind = ProviderKind::parse(&provider_kind);

    let mut provider = open_provider(account_id, user_id, app).await?;

    // Discover folders (task 4.1). Every folder is upserted so it stays visible
    // and toggle-able in settings, regardless of whether it's synced.
    let folders = provider
        .list_folders()
        .await
        .map_err(|error| normalize_provider_error(provider_kind, error))?;
    for folder in &folders {
        let default_sync_enabled = default_folder_sync_enabled(provider_kind, &folder.folder_type);
        sqlx::query(
            "INSERT INTO folders (account_id, name, full_path, folder_type, sync_enabled) VALUES (?, ?, ?, ?, ?) ON CONFLICT(account_id, full_path) DO UPDATE SET name = excluded.name, folder_type = excluded.folder_type",
        )
        .bind(account_id)
        .bind(&folder.name)
        .bind(&folder.full_path)
        .bind(&folder.folder_type)
        .bind(default_sync_enabled as i64)
        .execute(&db)
        .await?;
    }

    // Only sync folders the user kept enabled (sync_enabled defaults to 1).
    let enabled: std::collections::HashSet<String> = sqlx::query_scalar(
        "SELECT full_path FROM folders WHERE account_id = ? AND sync_enabled = 1",
    )
    .bind(account_id)
    .fetch_all(&db)
    .await
    .unwrap_or_default()
    .into_iter()
    .collect();
    let mut synced_folders: Vec<&crate::session::FolderInfo> = folders
        .iter()
        .filter(|f| enabled.contains(&f.full_path))
        .collect();
    synced_folders.sort_by_key(|f| folder_sync_priority(&f.folder_type));

    // Progress total: sum of server-reported message counts across synced
    // folders. A cheap status pre-pass gives a stable denominator before the
    // backfill starts inserting rows.
    let mut total: i64 = 0;
    for folder in &synced_folders {
        match provider.folder_status(&folder.full_path).await {
            Ok(status) => total += status.exists as i64,
            Err(error) => {
                if let Some(error) = oauth_reauthentication_error(provider_kind, &error) {
                    return Err(error.into());
                }
            }
        }
    }
    // Count over the same set `total` covers: synced (enabled) folders, live
    // messages only. Counting every folder or including soft-deleted rows made
    // `synced` exceed `total` (e.g. "519 / 226").
    let already: i64 = sqlx::query_scalar(db::queries::SYNCED_MESSAGE_COUNT_SQL)
        .bind(account_id)
        .fetch_one(&db)
        .await
        .unwrap_or(0);
    app.sync_manager()
        .set_progress(account_id, already, total)
        .await;
    publish_sync_status(account_id, user_id, app).await;

    // Sync each enabled folder
    for folder in &synced_folders {
        if let Err(e) = sync_folder(
            account_id,
            user_id,
            &folder.full_path,
            &body_sync_mode,
            provider_kind,
            total,
            provider.as_mut(),
            &db,
            app,
        )
        .await
        {
            if let Some(error) = oauth_reauthentication_error(provider_kind, e.as_ref()) {
                return Err(error.into());
            }
            warn!(
                "folder sync error: account={account_id} folder={} folder_type={} err={e}",
                folder.full_path, folder.folder_type
            );
        }
    }

    let _ = provider.close().await;
    Ok(())
}

fn normalize_provider_error(
    provider_kind: ProviderKind,
    error: ProviderError,
) -> Box<dyn std::error::Error + Send + Sync> {
    match oauth_reauthentication_error(provider_kind, &error) {
        Some(error) => error.into(),
        None => Box::new(error),
    }
}

fn oauth_reauthentication_error(
    provider_kind: ProviderKind,
    error: &(dyn std::error::Error + Send + Sync + 'static),
) -> Option<&'static str> {
    let ProviderError::Http { status: 401, .. } = error.downcast_ref::<ProviderError>()? else {
        return None;
    };

    match provider_kind {
        ProviderKind::GmailApi => Some("oauth_reauthentication_required:google"),
        ProviderKind::OutlookApi => Some("oauth_reauthentication_required:microsoft"),
        ProviderKind::Imap | ProviderKind::GmailImap => None,
    }
}

fn default_folder_sync_enabled(provider_kind: ProviderKind, folder_type: &str) -> bool {
    if provider_kind == ProviderKind::GmailApi {
        // Gmail API drafts are special resources and can return transport/body
        // decode errors through the messages endpoint. Keep the primary sync
        // focused on real mailbox traffic; Drafts can be enabled manually after
        // the API path grows first-class draft support.
        matches!(folder_type, "INBOX" | "SENT")
    } else if provider_kind == ProviderKind::GmailImap {
        // Gmail exposes labels as IMAP folders, including "[Gmail]/All Mail"
        // as ARCHIVE. Syncing every label by default duplicates messages across
        // folders and makes first backfill painfully slow. Keep the mailbox
        // useful immediately; custom labels and All Mail remain opt-in in
        // Settings > Accounts > Synced folders.
        matches!(folder_type, "INBOX" | "SENT" | "DRAFTS")
    } else {
        true
    }
}

fn folder_sync_priority(folder_type: &str) -> u8 {
    match folder_type {
        "INBOX" => 0,
        "DRAFTS" => 1,
        "SENT" => 2,
        "ARCHIVE" => 3,
        "CUSTOM" => 4,
        "SPAM" => 5,
        "TRASH" => 6,
        _ => 7,
    }
}

/// Load the connection config for an account, plus its provider kind. Shared by
/// the polling path ([`open_provider`]) and the IMAP IDLE task.
async fn load_provider_config(
    account_id: &str,
    user_id: &str,
    app: &Arc<dyn SyncAppState>,
) -> Result<(ProviderConfig, ProviderKind), Box<dyn std::error::Error + Send + Sync>> {
    let db = app.user_db(user_id).await?;
    let row: Option<(Vec<u8>, String, String, i64, String, Option<String>, String)> = sqlx::query_as(
        "SELECT credentials_encrypted, primary_email, imap_host, imap_port, imap_auth_scheme, imap_tls_cert, provider_kind FROM email_accounts WHERE id = ?",
    )
    .bind(account_id)
    .fetch_optional(&db)
    .await?;

    let (creds_enc, primary_email, host, port, auth_scheme, tls_cert, kind) =
        row.ok_or("account not found")?;
    let creds_bytes = app.credential_key().decrypt(&creds_enc)?;
    let creds: serde_json::Value = serde_json::from_slice(&creds_bytes)?;

    // Prefer a freshly refreshed OAuth token over the stored one (the stored
    // access token may be expired; the api layer refreshes and persists it).
    let oauth_access_token = match app
        .fresh_oauth_token(user_id, account_id)
        .await
        .map_err(std::io::Error::other)?
    {
        Some(token) => Some(token),
        None => creds["oauth_access_token"].as_str().map(|s| s.to_owned()),
    };

    let mut username = creds["imap_username"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&primary_email)
        .trim()
        .to_owned();
    if auth_scheme == "xoauth2" && username == "oauth@pending" {
        return Err(
            "OAuth account is still pending; reconnect it to store the real mailbox address".into(),
        );
    }
    if auth_scheme == "xoauth2" && username.is_empty() {
        return Err("OAuth account is missing an IMAP username; reconnect it".into());
    }
    username.make_ascii_lowercase();

    let config = ProviderConfig {
        host,
        port: port as u16,
        username,
        password: creds["imap_password"].as_str().unwrap_or("").to_owned(),
        oauth_access_token,
        auth_scheme,
        trusted_cert_der: crate::session::decode_trusted_cert(tls_cert.as_deref()),
        db: Some(db),
        account_id: account_id.to_owned(),
    };
    Ok((config, ProviderKind::parse(&kind)))
}

/// Open the account's mailbox backend (IMAP or one of the API providers,
/// chosen by `email_accounts.provider_kind`).
async fn open_provider(
    account_id: &str,
    user_id: &str,
    app: &Arc<dyn SyncAppState>,
) -> Result<Box<dyn MailProvider>, Box<dyn std::error::Error + Send + Sync>> {
    let (config, kind) = load_provider_config(account_id, user_id, app).await?;
    provider::connect(kind, &config)
        .await
        .map_err(|e| Box::new(e) as _)
}

/// Aborts the wrapped task when dropped — ties the IMAP IDLE task's lifetime to
/// its parent sync task, whether that ends gracefully or is aborted.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Per-account IMAP IDLE loop: holds a dedicated connection on INBOX and asks
/// the sync task to poll whenever the server reports activity. Reconnects with
/// exponential backoff; exits cleanly once the sync task's command channel is
/// gone.
async fn run_idle_task(
    account_id: String,
    user_id: String,
    app: Arc<dyn SyncAppState>,
    tx: mpsc::Sender<SyncCommand>,
) {
    // Refresh well under the RFC 2177 29-minute ceiling.
    const MAX_WAIT: time::Duration = time::Duration::from_secs(25 * 60);
    let mut backoff = time::Duration::from_secs(5);
    // The first connection follows the task's own initial sync, so no catch-up
    // poll is needed; reconnects do poll once to pick up anything missed.
    let mut catch_up = false;

    loop {
        match idle_session_loop(&account_id, &user_id, &app, &tx, MAX_WAIT, catch_up).await {
            Ok(()) => {
                info!("idle task stopping: account={account_id}");
                return;
            }
            Err(e) => {
                warn!(
                    "idle: account={account_id} connection error: {e}; reconnecting in {backoff:?}"
                );
                time::sleep(backoff).await;
                backoff = (backoff * 2).min(time::Duration::from_secs(300));
                catch_up = true;
            }
        }
    }
}

/// One IMAP IDLE connection's lifetime. Returns `Ok(())` when there is nothing
/// to idle on (non-IMAP) or the sync task has gone away; returns `Err` on any
/// connection error so the caller reconnects.
async fn idle_session_loop(
    account_id: &str,
    user_id: &str,
    app: &Arc<dyn SyncAppState>,
    tx: &mpsc::Sender<SyncCommand>,
    max_wait: time::Duration,
    catch_up: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (config, kind) = load_provider_config(account_id, user_id, app).await?;
    if !kind.syncs_over_imap() {
        return Ok(());
    }

    let mut session = crate::session::connect_imap(
        &config.host,
        config.port,
        &config.username,
        &config.password,
        config.oauth_access_token.as_deref(),
        &config.auth_scheme,
        config.trusted_cert_der.as_deref(),
    )
    .await?;
    crate::session::select_folder(&mut session, "INBOX").await?;
    info!("idle: account={account_id} watching INBOX");

    // Reconnecting after a drop — poll once to pick up mail that arrived while
    // the IDLE connection was down.
    if catch_up && tx.send(SyncCommand::ForcePoll).await.is_err() {
        let _ = session.logout().await;
        return Ok(());
    }

    loop {
        let (next, activity) = crate::session::idle_once(session, max_wait).await?;
        session = next;
        if activity {
            info!("idle: account={account_id} reported activity, triggering poll");
            if tx.send(SyncCommand::ForcePoll).await.is_err() {
                // Sync task is gone — stop without reconnecting.
                let _ = session.logout().await;
                return Ok(());
            }
        }
        // On timeout, just re-issue IDLE to keep the session under 29 minutes.
    }
}

#[allow(clippy::too_many_arguments)]
async fn sync_folder(
    account_id: &str,
    user_id: &str,
    folder_path: &str,
    body_sync_mode: &str,
    provider_kind: ProviderKind,
    total: i64,
    provider: &mut dyn MailProvider,
    db: &sqlx::SqlitePool,
    app: &Arc<dyn SyncAppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Get stored folder info
    let folder_row: Option<(String, Option<i64>, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT id, uidvalidity, last_uid, folder_type FROM folders WHERE account_id = ? AND full_path = ?",
    )
    .bind(account_id)
    .bind(folder_path)
    .fetch_optional(db)
    .await?;

    let (folder_id, stored_uidvalidity, last_uid, folder_type) =
        folder_row.ok_or("folder not found")?;

    // Spam and trash get new mail constantly; never push-notify for them.
    let notify_allowed = !matches!(folder_type.as_deref(), Some("SPAM") | Some("TRASH"));

    // Folder status (IMAP: SELECT; APIs: metadata lookup)
    let server_uidvalidity = provider.folder_status(folder_path).await?.uidvalidity;

    // Handle UIDVALIDITY change — purge and full re-sync (task 4.5)
    if let Some(sv) = stored_uidvalidity {
        if sv != server_uidvalidity as i64 {
            warn!(
                "UIDVALIDITY changed: account={account_id} folder={folder_path} old={sv} new={server_uidvalidity}; purging and re-syncing"
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

    // Incremental sync in bounded UID chunks (task 4.5). Fetching the whole
    // `uid_start:*` range at once buffers every message before a single row is
    // written — on large mailboxes (tens of thousands of messages) that stalls
    // or times out, so nothing is committed. Chunking keeps each fetch small,
    // commits progress per chunk, and lets messages stream into the UI.
    let uid_chunk: u32 = if provider_kind == ProviderKind::GmailApi {
        200
    } else {
        500
    };
    // IMAP UIDs are u32; last_uid is stored as i64. Work in u32 for the walk.
    let prev_last_uid: u32 = last_uid.unwrap_or(0).clamp(0, u32::MAX as i64) as u32;
    let uid_start: u32 = prev_last_uid + 1;
    // First-ever backfill of this folder (no prior cursor): every message is
    // "new", so suppress push notifications — they should only fire for mail
    // that actually arrives after the initial sync.
    let initial_sync = prev_last_uid == 0;

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
        let chunk_end = chunk_start.saturating_add(uid_chunk - 1).min(highest);
        let uid_set = format!("{}:{}", chunk_start, chunk_end);

        let mut messages = if body_sync_mode == "full" {
            // Full sync: fetch complete raw messages (task 4.3)
            provider.fetch_full(folder_path, &uid_set).await?
        } else {
            // Lazy sync: fetch headers only (task 4.2)
            provider.fetch_headers(folder_path, &uid_set).await?
        };
        messages.sort_by_key(|msg| msg.uid);
        let mut phishing_batch: Vec<(String, &[u8])> = Vec::new();

        for msg in &messages {
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
            "INSERT INTO messages (account_id, folder_id, uid, message_id_header, thread_id, in_reply_to, \"references\", list_id, subject, subject_normalized, snippet, from_addr, to_addrs, cc_addrs, date, internal_date, is_read, is_flagged, is_deleted) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(folder_id, uid) DO UPDATE SET thread_id = excluded.thread_id, is_read = excluded.is_read, is_flagged = excluded.is_flagged, is_deleted = MAX(excluded.is_deleted, messages.is_deleted), subject = excluded.subject, subject_normalized = excluded.subject_normalized, snippet = excluded.snippet, from_addr = excluded.from_addr, to_addrs = excluded.to_addrs, cc_addrs = excluded.cc_addrs RETURNING id",
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
        .bind(msg.is_deleted as i64)
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
                    let blob_key =
                        mailquill_core::blob::blob_key_body(account_id, msg.uid, internal_date);

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

                    if parsed.has_calendar {
                        process_calendar_parts(db, &msg_db_id, &parsed.calendar_parts).await;
                    }

                    // FTS index (task 4.9)
                    let body_text = parsed.text.as_deref().unwrap_or("");
                    // FTS5 rejects ON CONFLICT/UPSERT on a virtual table, so use
                    // INSERT OR REPLACE to refresh the row keyed by rowid.
                    let _ = sqlx::query(
                    "INSERT OR REPLACE INTO messages_fts(rowid, subject, from_addr, body_text) VALUES ((SELECT rowid FROM messages WHERE id = ?), ?, ?, ?)",
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
                    phishing_batch.push((msg_db_id.clone(), raw));
                }
            }

            // Update FTS for lazy sync with subject + from_addr (no body text)
            if body_sync_mode == "lazy" {
                // OR IGNORE keeps any existing row (with body) intact; FTS5
                // does not support ON CONFLICT here.
                let _ = sqlx::query(
                "INSERT OR IGNORE INTO messages_fts(rowid, subject, from_addr, body_text) VALUES ((SELECT rowid FROM messages WHERE id = ?), ?, ?, '')",
            )
            .bind(&msg_db_id)
            .bind(&msg.subject)
            .bind(&msg.from_addr)
            .execute(db)
            .await;
            }

            let should_notify = is_new_message
                && notify_allowed
                && !msg.is_seen
                && !msg.is_deleted
                && !initial_sync;

            if should_notify {
                let notification = NewMessageNotification {
                    message_id: msg_db_id.clone(),
                    account_id: account_id.to_owned(),
                    account_name: account_name.clone(),
                    sender: msg.from_addr.clone(),
                    subject: msg.subject.clone(),
                };
                if let Err(e) = app.notify_new_message(user_id, notification).await {
                    warn!(
                        "web push notification failed: account={account_id} folder={folder_path} message={msg_db_id} err={e}"
                    );
                }
            }
        }

        let phishing_inputs: Vec<(&str, &[u8])> = phishing_batch
            .iter()
            .map(|(message_id, raw)| (message_id.as_str(), *raw))
            .collect();
        phishing::analyse_and_store_batch(db, &phishing_inputs).await;

        // Persist progress after each chunk (task 4.14) so messages stream into
        // the UI and an interrupted sync resumes from the last committed UID.
        // Only advance through a contiguous run of locally stored UIDs: API
        // providers may return partial chunks during quota/backpressure events,
        // and moving past a gap would permanently skip that message.
        let chunk_uids: Vec<i64> = sqlx::query_scalar(
            "SELECT uid FROM messages WHERE folder_id = ? AND uid BETWEEN ? AND ? ORDER BY uid",
        )
        .bind(&folder_id)
        .bind(chunk_start as i64)
        .bind(chunk_end as i64)
        .fetch_all(db)
        .await
        .unwrap_or_default();
        max_uid = contiguous_uid(max_uid, &chunk_uids);

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
        // Match the denominator: enabled folders, live messages only.
        let synced: i64 = sqlx::query_scalar(db::queries::SYNCED_MESSAGE_COUNT_SQL)
            .bind(account_id)
            .fetch_one(db)
            .await
            .unwrap_or(0);
        app.sync_manager()
            .set_progress(account_id, synced, total)
            .await;
        publish_sync_status(account_id, user_id, app).await;

        if chunk_end >= highest {
            break;
        }
        chunk_start = chunk_end + 1;
    }

    Ok(())
}

fn contiguous_uid(current: i64, sorted_uids: &[i64]) -> i64 {
    let mut next = current;
    for uid in sorted_uids {
        if *uid <= next {
            continue;
        }
        if *uid == next + 1 {
            next = *uid;
        } else {
            break;
        }
    }
    next
}

async fn process_calendar_parts(db: &sqlx::SqlitePool, message_id: &str, parts: &[String]) {
    for raw_ical in parts {
        let method = calendar_sync::parse_method(raw_ical).unwrap_or_else(|| "PUBLISH".to_owned());
        let events = calendar_sync::parse_icalendar_events(raw_ical);
        for event in events {
            if event.uid.is_empty() {
                continue;
            }
            let attendees = event
                .attendees_json
                .clone()
                .unwrap_or_else(|| "[]".to_owned());
            match method.as_str() {
                "REQUEST" => {
                    let _ = sqlx::query(
                        "DELETE FROM meeting_invitations WHERE message_id = ? AND uid = ?",
                    )
                    .bind(message_id)
                    .bind(&event.uid)
                    .execute(db)
                    .await;
                    let _ = sqlx::query(
                        "INSERT INTO meeting_invitations \
                         (message_id, method, uid, summary, start_dt, end_dt, organizer_email, attendees, user_rsvp_status, raw_ical, ms_teams_url, updated_at) \
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, datetime('now'))",
                    )
                    .bind(message_id)
                    .bind(&method)
                    .bind(&event.uid)
                    .bind(&event.title)
                    .bind(&event.starts_at)
                    .bind(&event.ends_at)
                    .bind(&event.organizer_email)
                    .bind(&attendees)
                    .bind(raw_ical)
                    .bind(&event.ms_teams_url)
                    .execute(db)
                    .await;
                }
                "CANCEL" => {
                    let _ = sqlx::query(
                        "UPDATE calendar_events SET status = 'cancelled', updated_at = datetime('now') WHERE uid = ?",
                    )
                    .bind(&event.uid)
                    .execute(db)
                    .await;
                    let _ = sqlx::query(
                        "DELETE FROM meeting_invitations WHERE message_id = ? AND uid = ?",
                    )
                    .bind(message_id)
                    .bind(&event.uid)
                    .execute(db)
                    .await;
                    let _ = sqlx::query(
                        "INSERT INTO meeting_invitations \
                         (message_id, method, uid, summary, start_dt, end_dt, organizer_email, attendees, user_rsvp_status, raw_ical, ms_teams_url, updated_at) \
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'cancelled', ?, ?, datetime('now'))",
                    )
                    .bind(message_id)
                    .bind(&method)
                    .bind(&event.uid)
                    .bind(&event.title)
                    .bind(&event.starts_at)
                    .bind(&event.ends_at)
                    .bind(&event.organizer_email)
                    .bind(&attendees)
                    .bind(raw_ical)
                    .bind(&event.ms_teams_url)
                    .execute(db)
                    .await;
                }
                "REPLY" => {
                    let _ = sqlx::query(
                        "UPDATE calendar_events SET attendees = ?, updated_at = datetime('now') WHERE uid = ?",
                    )
                    .bind(&attendees)
                    .bind(&event.uid)
                    .execute(db)
                    .await;
                }
                _ => {}
            }
        }
    }
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
        let mut tx = db.begin().await?;
        for (uid, seen, flagged, deleted) in flags {
            // Hide messages the server has marked \Deleted (e.g. left behind by a
            // copy-then-flag move) without un-hiding a locally deleted row.
            sqlx::query(
                "UPDATE messages SET is_read = ?, is_flagged = ?, is_deleted = MAX(?, is_deleted) WHERE folder_id = ? AND uid = ?",
            )
            .bind(seen as i64)
            .bind(flagged as i64)
            .bind(deleted as i64)
            .bind(folder_id)
            .bind(uid as i64)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
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
    let dest = resolve_dest_folder(account_id, user_id, dest_folder, app).await;
    let mut provider = open_provider(account_id, user_id, app).await?;
    provider.move_message(src_folder, uid, &dest).await?;
    let _ = provider.close().await;
    Ok(())
}

/// Map a logical destination ("Trash"/"Archive"/"Spam") to the account's real
/// folder. IMAP servers localize these ("Papierkorb", "INBOX.Trash"), so a
/// literal name won't match — we look the path up by folder_type instead. Any
/// other value is treated as an explicit folder path and passed through.
async fn resolve_dest_folder(
    account_id: &str,
    user_id: &str,
    dest: &str,
    app: &Arc<dyn SyncAppState>,
) -> String {
    let folder_type = match dest {
        "Trash" => "TRASH",
        "Archive" => "ARCHIVE",
        "Spam" | "Junk" => "SPAM",
        _ => return dest.to_owned(),
    };
    if let Ok(db) = app.user_db(user_id).await {
        if let Ok(Some(path)) = sqlx::query_scalar::<_, String>(
            "SELECT full_path FROM folders WHERE account_id = ? AND folder_type = ? ORDER BY full_path LIMIT 1",
        )
        .bind(account_id)
        .bind(folder_type)
        .fetch_optional(&db)
        .await
        {
            return path;
        }
    }
    dest.to_owned()
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

#[cfg(test)]
mod tests {
    use super::{
        default_folder_sync_enabled, folder_sync_priority, oauth_reauthentication_error,
        ProviderError, ProviderKind,
    };

    #[test]
    fn gmail_defaults_skip_archive_and_custom_labels() {
        assert!(default_folder_sync_enabled(
            ProviderKind::GmailImap,
            "INBOX"
        ));
        assert!(default_folder_sync_enabled(ProviderKind::GmailImap, "SENT"));
        assert!(default_folder_sync_enabled(
            ProviderKind::GmailImap,
            "DRAFTS"
        ));
        assert!(!default_folder_sync_enabled(
            ProviderKind::GmailImap,
            "ARCHIVE"
        ));
        assert!(!default_folder_sync_enabled(
            ProviderKind::GmailImap,
            "CUSTOM"
        ));
        assert!(!default_folder_sync_enabled(
            ProviderKind::GmailApi,
            "CUSTOM"
        ));
        assert!(!default_folder_sync_enabled(
            ProviderKind::GmailApi,
            "DRAFTS"
        ));
        assert!(default_folder_sync_enabled(ProviderKind::Imap, "CUSTOM"));
    }

    #[test]
    fn sync_priority_starts_with_inbox() {
        assert!(folder_sync_priority("INBOX") < folder_sync_priority("SENT"));
        assert!(folder_sync_priority("SENT") < folder_sync_priority("CUSTOM"));
        assert!(folder_sync_priority("CUSTOM") < folder_sync_priority("TRASH"));
    }

    #[test]
    fn contiguous_uid_stops_before_gaps() {
        assert_eq!(super::contiguous_uid(10, &[11, 12, 14, 15]), 12);
        assert_eq!(super::contiguous_uid(10, &[8, 10, 11, 12]), 12);
        assert_eq!(super::contiguous_uid(10, &[12, 13]), 10);
    }

    #[test]
    fn api_unauthorized_errors_require_oauth_reauthentication() {
        let unauthorized = ProviderError::Http {
            status: 401,
            body: "invalid credentials".into(),
        };

        assert_eq!(
            oauth_reauthentication_error(ProviderKind::GmailApi, &unauthorized),
            Some("oauth_reauthentication_required:google")
        );
        assert_eq!(
            oauth_reauthentication_error(ProviderKind::OutlookApi, &unauthorized),
            Some("oauth_reauthentication_required:microsoft")
        );
        assert_eq!(
            oauth_reauthentication_error(ProviderKind::Imap, &unauthorized),
            None
        );
    }

    #[test]
    fn non_authentication_http_errors_remain_folder_errors() {
        let forbidden = ProviderError::Http {
            status: 403,
            body: "quota exceeded".into(),
        };

        assert_eq!(
            oauth_reauthentication_error(ProviderKind::GmailApi, &forbidden),
            None
        );
    }
}
