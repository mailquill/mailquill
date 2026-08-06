use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{error::AppError, middleware::UserId, state::AppState, tls::TlsDecision, validate};

#[derive(Deserialize)]
pub struct AddAccountRequest {
    display_name: String,
    primary_email: String,
    imap_host: String,
    imap_port: u16,
    imap_username: String,
    imap_password: String,
    imap_auth_scheme: Option<String>,
    smtp_host: String,
    smtp_port: u16,
    smtp_username: String,
    smtp_password: String,
    smtp_auth_scheme: Option<String>,
    body_sync_mode: Option<String>,
    sync_interval_secs: Option<i64>,
    /// Sync strategy: 'idle' (IMAP push) or 'interval' (poll). Default 'idle'.
    sync_mode: Option<String>,
    carddav_url: Option<String>,
    caldav_url: Option<String>,
    caldav_accept_invalid_tls: Option<bool>,
    /// User-approved TLS trust exceptions: base64 DER certificate per service.
    imap_tls_cert: Option<String>,
    smtp_tls_cert: Option<String>,
    tls_decision: Option<TlsDecision>,
    /// Mailbox backend: imap (default), gmail_api, gmail_imap, outlook_api.
    provider_kind: Option<String>,
    pgp_key_id: Option<String>,
    sign_by_default: Option<bool>,
    /// Contact synchronization is optional and never blocks mailbox creation.
    contacts_enabled: Option<bool>,
    /// Account colour (`#RRGGBB`); NULL lets the client derive one from the id.
    color: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateAccountRequest {
    display_name: Option<String>,
    imap_host: Option<String>,
    imap_port: Option<u16>,
    imap_username: Option<String>,
    imap_password: Option<String>,
    imap_auth_scheme: Option<String>,
    smtp_host: Option<String>,
    smtp_port: Option<u16>,
    smtp_username: Option<String>,
    smtp_password: Option<String>,
    smtp_auth_scheme: Option<String>,
    body_sync_mode: Option<String>,
    sync_interval_secs: Option<i64>,
    sync_mode: Option<String>,
    carddav_url: Option<String>,
    caldav_url: Option<String>,
    caldav_accept_invalid_tls: Option<bool>,
    imap_tls_cert: Option<String>,
    smtp_tls_cert: Option<String>,
    tls_decision: Option<TlsDecision>,
    pgp_key_id: Option<String>,
    sign_by_default: Option<bool>,
    contacts_enabled: Option<bool>,
    color: Option<String>,
}

#[derive(Deserialize)]
pub struct ReorderAccountsRequest {
    /// Every account id of the user, in the desired sidebar order.
    account_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContactCapabilitySummary {
    source_id: String,
    provider: String,
    state: String,
    reason: Option<String>,
    enabled: bool,
    last_synced_at: Option<String>,
    cache_retained: bool,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct AccountResponse {
    id: String,
    display_name: String,
    primary_email: String,
    imap_host: String,
    imap_port: i64,
    imap_auth_scheme: String,
    smtp_host: String,
    smtp_port: i64,
    smtp_auth_scheme: String,
    body_sync_mode: String,
    sync_interval_secs: i64,
    sync_mode: String,
    provider_kind: String,
    created_at: String,
    carddav_url: Option<String>,
    caldav_url: Option<String>,
    caldav_accept_invalid_tls: bool,
    pgp_key_id: Option<String>,
    sign_by_default: bool,
    color: Option<String>,
    sort_order: i64,
    #[sqlx(skip)]
    contacts: Option<ContactCapabilitySummary>,
}

#[derive(Deserialize)]
pub struct AddAliasRequest {
    email: String,
    display_name: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateAliasRequest {
    display_name: Option<String>,
}

pub async fn add_account(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<AddAccountRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Zero-trust: validate every field server-side before touching crypto/DB/network.
    validate::text("display_name", &req.display_name, 200)?;
    validate::email("primary_email", &req.primary_email)?;
    validate::host("imap_host", &req.imap_host)?;
    validate::port("imap_port", req.imap_port)?;
    validate::text("imap_username", &req.imap_username, 320)?;
    validate::text("imap_password", &req.imap_password, 4096)?;
    validate::host("smtp_host", &req.smtp_host)?;
    validate::port("smtp_port", req.smtp_port)?;
    validate::text("smtp_username", &req.smtp_username, 320)?;
    validate::text("smtp_password", &req.smtp_password, 4096)?;
    if let Some(s) = &req.imap_auth_scheme {
        validate::one_of("imap_auth_scheme", s, validate::AUTH_SCHEMES)?;
    }
    if let Some(s) = &req.smtp_auth_scheme {
        validate::one_of("smtp_auth_scheme", s, validate::AUTH_SCHEMES)?;
    }
    if let Some(s) = req.sync_interval_secs {
        validate::range_i64("sync_interval_secs", s, 30, 86_400)?;
    }
    if let Some(u) = req.carddav_url.as_deref().filter(|s| !s.is_empty()) {
        validate::http_url("carddav_url", u)?;
    }
    if let Some(u) = req.caldav_url.as_deref().filter(|s| !s.is_empty()) {
        validate::http_url("caldav_url", u)?;
    }
    let imap_tls_cert = req.imap_tls_cert.as_deref().filter(|s| !s.is_empty());
    let smtp_tls_cert = req.smtp_tls_cert.as_deref().filter(|s| !s.is_empty());
    let imap_tls_cert_der = imap_tls_cert
        .map(|s| validate::b64_cert("imap_tls_cert", s))
        .transpose()?;
    if let Some(s) = smtp_tls_cert {
        validate::b64_cert("smtp_tls_cert", s)?;
    }

    let body_sync_mode = req.body_sync_mode.as_deref().unwrap_or("lazy");
    validate::one_of("body_sync_mode", body_sync_mode, validate::BODY_SYNC_MODES)?;

    let provider_kind = req.provider_kind.as_deref().unwrap_or("imap");
    validate::one_of(
        "provider_kind",
        provider_kind,
        &["imap", "gmail_api", "gmail_imap", "outlook_api"],
    )?;

    let sync_mode = req.sync_mode.as_deref().unwrap_or("idle");
    validate::one_of("sync_mode", sync_mode, validate::SYNC_MODES)?;

    if let Some(c) = &req.color {
        validate::hex_color("color", c)?;
    }

    // Reject duplicates before the (slow) IMAP connection test; re-adding an
    // existing mailbox would otherwise silently create a second account.
    let user_db = state.user_db_pool.get(&user.0).await?;
    let duplicate: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM email_accounts WHERE lower(primary_email) = lower(?)",
    )
    .bind(&req.primary_email)
    .fetch_one(&user_db)
    .await?;
    if duplicate > 0 {
        return Err(AppError::Conflict(
            "an account with this email address already exists".into(),
        ));
    }

    // Encode credentials as JSON then encrypt
    let creds = serde_json::json!({
        "imap_username": req.imap_username,
        "imap_password": req.imap_password,
        "smtp_username": req.smtp_username,
        "smtp_password": req.smtp_password,
    });
    let creds_bytes = serde_json::to_vec(&creds).unwrap();
    let encrypted = state
        .credential_key
        .encrypt(&creds_bytes)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Test IMAP connection before persisting
    let imap_host = req.imap_host.clone();
    let imap_port = req.imap_port;
    let imap_user = req.imap_username.clone();
    let imap_pass = req.imap_password.clone();
    let auth_scheme = req
        .imap_auth_scheme
        .clone()
        .unwrap_or_else(|| "plain".into());

    let tls_retry_permitted = req
        .tls_decision
        .map(TlsDecision::permits_retry)
        .unwrap_or(imap_tls_cert_der.is_some());
    let persist_tls_exception = req
        .tls_decision
        .map(TlsDecision::persists_exception)
        .unwrap_or(imap_tls_cert_der.is_some());
    if let Err(e) = mail_sync::test_imap_connection(
        &imap_host,
        imap_port,
        &imap_user,
        &imap_pass,
        &auth_scheme,
        tls_retry_permitted
            .then_some(imap_tls_cert_der.as_deref())
            .flatten(),
    )
    .await
    {
        // TLS failures carry the presented certificate so the UI can offer a
        // Thunderbird-style trust exception; the user retries with the cert.
        if matches!(e, mail_sync::SessionError::Tls(_)) {
            if let Some(der) = crate::routes::discover::fetch_peer_cert(&imap_host, imap_port).await
            {
                return Ok((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(tls_untrusted_json(
                        &e.to_string(),
                        &imap_host,
                        imap_port,
                        &der,
                    )),
                )
                    .into_response());
            }
        }
        return Err(AppError::Unprocessable(e.to_string()));
    }

    let account_id: String = sqlx::query_scalar(
        "INSERT INTO email_accounts (display_name, primary_email, imap_host, imap_port, imap_auth_scheme, smtp_host, smtp_port, smtp_auth_scheme, credentials_encrypted, body_sync_mode, sync_interval_secs, sync_mode, carddav_url, caldav_url, caldav_accept_invalid_tls, imap_tls_cert, smtp_tls_cert, provider_kind, pgp_key_id, sign_by_default, color, sort_order) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, (SELECT COALESCE(MAX(sort_order) + 1, 0) FROM email_accounts)) RETURNING id",
    )
    .bind(&req.display_name)
    .bind(&req.primary_email)
    .bind(&req.imap_host)
    .bind(req.imap_port as i64)
    .bind(req.imap_auth_scheme.as_deref().unwrap_or("plain"))
    .bind(&req.smtp_host)
    .bind(req.smtp_port as i64)
    .bind(req.smtp_auth_scheme.as_deref().unwrap_or("plain"))
    .bind(&encrypted)
    .bind(body_sync_mode)
    .bind(req.sync_interval_secs.unwrap_or(300))
    .bind(sync_mode)
    .bind(req.carddav_url.as_deref().filter(|s| !s.is_empty()))
    .bind(req.caldav_url.as_deref().filter(|s| !s.is_empty()))
    .bind(req.caldav_accept_invalid_tls.unwrap_or(false))
    .bind(persist_tls_exception.then_some(imap_tls_cert).flatten())
    .bind(persist_tls_exception.then_some(smtp_tls_cert).flatten())
    .bind(provider_kind)
    .bind(&req.pgp_key_id)
    .bind(req.sign_by_default.unwrap_or(false))
    .bind(&req.color)
    .fetch_one(&user_db)
    .await?;

    reconcile_and_start_contacts(
        &state,
        &user.0,
        &user_db,
        &account_id,
        Some(req.contacts_enabled.unwrap_or(true)),
    )
    .await;

    // Kick off initial sync
    state
        .sync_manager
        .start_account(account_id.clone(), user.0.clone(), Arc::new(state.clone()))
        .await;

    let row = get_account_row(&user_db, &account_id).await?;
    Ok((StatusCode::CREATED, Json(row)).into_response())
}

pub async fn list_accounts(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let mut rows: Vec<AccountResponse> = sqlx::query_as(
        "SELECT id, display_name, primary_email, imap_host, imap_port, imap_auth_scheme, smtp_host, smtp_port, smtp_auth_scheme, body_sync_mode, sync_interval_secs, sync_mode, provider_kind, created_at, carddav_url, caldav_url, caldav_accept_invalid_tls, pgp_key_id, sign_by_default, color, sort_order FROM email_accounts ORDER BY sort_order, created_at",
    )
    .fetch_all(&user_db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;
    for row in &mut rows {
        row.contacts = contact_capability(&user_db, &row.id).await?;
    }

    Ok(Json(rows))
}

pub async fn get_account(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let row = get_account_row(&user_db, &account_id).await?;
    Ok(Json(row))
}

pub async fn update_account(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
    Json(req): Json<UpdateAccountRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Zero-trust: validate every provided field before any work.
    if let Some(v) = &req.display_name {
        validate::text("display_name", v, 200)?;
    }
    if let Some(v) = &req.imap_host {
        validate::host("imap_host", v)?;
    }
    if let Some(p) = req.imap_port {
        validate::port("imap_port", p)?;
    }
    if let Some(s) = &req.imap_auth_scheme {
        validate::one_of("imap_auth_scheme", s, validate::AUTH_SCHEMES)?;
    }
    if let Some(v) = &req.imap_username {
        validate::text("imap_username", v, 320)?;
    }
    if let Some(v) = &req.imap_password {
        validate::text("imap_password", v, 4096)?;
    }
    if let Some(v) = &req.smtp_host {
        validate::host("smtp_host", v)?;
    }
    if let Some(p) = req.smtp_port {
        validate::port("smtp_port", p)?;
    }
    if let Some(s) = &req.smtp_auth_scheme {
        validate::one_of("smtp_auth_scheme", s, validate::AUTH_SCHEMES)?;
    }
    if let Some(v) = &req.smtp_username {
        validate::text("smtp_username", v, 320)?;
    }
    if let Some(v) = &req.smtp_password {
        validate::text("smtp_password", v, 4096)?;
    }
    if let Some(m) = &req.body_sync_mode {
        validate::one_of("body_sync_mode", m, validate::BODY_SYNC_MODES)?;
    }
    if let Some(s) = req.sync_interval_secs {
        validate::range_i64("sync_interval_secs", s, 30, 86_400)?;
    }
    if let Some(m) = &req.sync_mode {
        validate::one_of("sync_mode", m, validate::SYNC_MODES)?;
    }
    if let Some(c) = &req.color {
        validate::hex_color("color", c)?;
    }
    let request_imap_cert = req
        .imap_tls_cert
        .as_deref()
        .filter(|certificate| !certificate.is_empty());
    let request_smtp_cert = req
        .smtp_tls_cert
        .as_deref()
        .filter(|certificate| !certificate.is_empty());
    let request_imap_cert_der = request_imap_cert
        .map(|certificate| validate::b64_cert("imap_tls_cert", certificate))
        .transpose()?;
    if let Some(certificate) = request_smtp_cert {
        validate::b64_cert("smtp_tls_cert", certificate)?;
    }
    // Empty string clears the URL (COALESCE keeps old only on NULL, so empty is allowed through).
    if let Some(u) = req.carddav_url.as_deref().filter(|s| !s.is_empty()) {
        validate::http_url("carddav_url", u)?;
    }
    if let Some(u) = req.caldav_url.as_deref().filter(|s| !s.is_empty()) {
        validate::http_url("caldav_url", u)?;
    }

    let user_db = state.user_db_pool.get(&user.0).await?;

    // Check ownership (returns 404 for both missing and wrong owner — D2)
    let existing: Option<(String, Vec<u8>)> =
        sqlx::query_as("SELECT id, credentials_encrypted FROM email_accounts WHERE id = ?")
            .bind(&account_id)
            .fetch_optional(&user_db)
            .await?;
    let (_id, old_creds_enc) = existing.ok_or(AppError::NotFound)?;

    // Decrypt old credentials to patch
    let old_creds_bytes = state
        .credential_key
        .decrypt(&old_creds_enc)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let mut creds: serde_json::Value =
        serde_json::from_slice(&old_creds_bytes).map_err(|e| AppError::Internal(e.to_string()))?;

    if let Some(u) = &req.imap_username {
        creds["imap_username"] = json!(u);
    }
    if let Some(p) = &req.imap_password {
        creds["imap_password"] = json!(p);
    }
    if let Some(u) = &req.smtp_username {
        creds["smtp_username"] = json!(u);
    }
    if let Some(p) = &req.smtp_password {
        creds["smtp_password"] = json!(p);
    }

    let new_creds_bytes = serde_json::to_vec(&creds).unwrap();
    let new_encrypted = state
        .credential_key
        .encrypt(&new_creds_bytes)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Re-test IMAP if credentials or host changed
    let test_imap_host: Option<String> = if let Some(h) = req.imap_host.clone() {
        Some(h)
    } else {
        sqlx::query_scalar::<_, String>("SELECT imap_host FROM email_accounts WHERE id = ?")
            .bind(&account_id)
            .fetch_optional(&user_db)
            .await?
    };
    let imap_user = creds["imap_username"].as_str().unwrap_or("").to_owned();
    let imap_pass = creds["imap_password"].as_str().unwrap_or("").to_owned();

    if let Some(host) = &test_imap_host {
        if let Some(port) = req.imap_port {
            let scheme = req
                .imap_auth_scheme
                .clone()
                .unwrap_or_else(|| "plain".into());
            let stored_tls_cert: Option<String> =
                sqlx::query_scalar("SELECT imap_tls_cert FROM email_accounts WHERE id = ?")
                    .bind(&account_id)
                    .fetch_one(&user_db)
                    .await?;
            let stored_trusted =
                mail_sync::session::decode_trusted_cert(stored_tls_cert.as_deref());
            let retry_permitted = req
                .tls_decision
                .map(TlsDecision::permits_retry)
                .unwrap_or(request_imap_cert_der.is_some());
            let trusted = if retry_permitted {
                request_imap_cert_der
                    .as_deref()
                    .or(stored_trusted.as_deref())
            } else {
                stored_trusted.as_deref()
            };
            if let Err(error) = mail_sync::test_imap_connection(
                host, port, &imap_user, &imap_pass, &scheme, trusted,
            )
            .await
            {
                if matches!(error, mail_sync::SessionError::Tls(_)) {
                    if let Some(der) = crate::routes::discover::fetch_peer_cert(host, port).await {
                        return Ok((
                            StatusCode::UNPROCESSABLE_ENTITY,
                            Json(tls_untrusted_json(&error.to_string(), host, port, &der)),
                        )
                            .into_response());
                    }
                }
                return Err(AppError::Unprocessable(error.to_string()));
            }
        }
    }

    let persist_tls_exception = req
        .tls_decision
        .map(TlsDecision::persists_exception)
        .unwrap_or(request_imap_cert.is_some());

    sqlx::query(
        "UPDATE email_accounts SET display_name = COALESCE(?, display_name), imap_host = COALESCE(?, imap_host), imap_port = COALESCE(?, imap_port), imap_auth_scheme = COALESCE(?, imap_auth_scheme), smtp_host = COALESCE(?, smtp_host), smtp_port = COALESCE(?, smtp_port), smtp_auth_scheme = COALESCE(?, smtp_auth_scheme), credentials_encrypted = ?, body_sync_mode = COALESCE(?, body_sync_mode), sync_interval_secs = COALESCE(?, sync_interval_secs), sync_mode = COALESCE(?, sync_mode), carddav_url = COALESCE(?, carddav_url), caldav_url = COALESCE(?, caldav_url), caldav_accept_invalid_tls = COALESCE(?, caldav_accept_invalid_tls), imap_tls_cert = COALESCE(?, imap_tls_cert), smtp_tls_cert = COALESCE(?, smtp_tls_cert), pgp_key_id = COALESCE(?, pgp_key_id), sign_by_default = COALESCE(?, sign_by_default), color = COALESCE(?, color) WHERE id = ?",
    )
    .bind(req.display_name.as_deref())
    .bind(req.imap_host.as_deref())
    .bind(req.imap_port.map(|p| p as i64))
    .bind(req.imap_auth_scheme.as_deref())
    .bind(req.smtp_host.as_deref())
    .bind(req.smtp_port.map(|p| p as i64))
    .bind(req.smtp_auth_scheme.as_deref())
    .bind(&new_encrypted)
    .bind(req.body_sync_mode.as_deref())
    .bind(req.sync_interval_secs)
    .bind(req.sync_mode.as_deref())
    .bind(req.carddav_url.as_deref())
    .bind(req.caldav_url.as_deref())
    .bind(req.caldav_accept_invalid_tls)
    .bind(persist_tls_exception.then_some(request_imap_cert).flatten())
    .bind(persist_tls_exception.then_some(request_smtp_cert).flatten())
    .bind(req.pgp_key_id.as_deref())
    .bind(req.sign_by_default)
    .bind(req.color.as_deref())
    .bind(&account_id)
    .execute(&user_db)
    .await?;

    // Restart the sync task so a changed sync_mode / interval takes effect now
    // (the IDLE task and poll ticker are set up once at task start).
    state
        .sync_manager
        .start_account(account_id.clone(), user.0.clone(), Arc::new(state.clone()))
        .await;

    reconcile_and_start_contacts(&state, &user.0, &user_db, &account_id, req.contacts_enabled)
        .await;

    let row = get_account_row(&user_db, &account_id).await?;
    Ok(Json(row).into_response())
}

fn tls_untrusted_json(error: &str, host: &str, port: u16, der: &[u8]) -> serde_json::Value {
    use base64::Engine;

    json!({
        "error": error,
        "code": "tls_untrusted",
        "cert": {
            "host": host,
            "port": port,
            "fingerprint_sha256": crate::routes::discover::cert_fingerprint_sha256(der),
            "der_base64": base64::engine::general_purpose::STANDARD.encode(der),
        },
    })
}

pub async fn delete_account(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM email_accounts WHERE id = ?")
        .bind(&account_id)
        .fetch_optional(&user_db)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    // Cancel sync task before deleting DB rows
    state.sync_manager.stop_account(&account_id).await;
    let managed_source: Option<String> =
        sqlx::query_scalar("SELECT id FROM contact_accounts WHERE email_account_id = ?")
            .bind(&account_id)
            .fetch_optional(&user_db)
            .await?;
    if let Some(source_id) = managed_source {
        state.contact_sync_manager.stop_account(&source_id).await;
    }

    // Cascade delete handled by FK ON DELETE CASCADE
    sqlx::query("DELETE FROM email_accounts WHERE id = ?")
        .bind(&account_id)
        .execute(&user_db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn sync_status(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM email_accounts WHERE id = ?")
        .bind(&account_id)
        .fetch_optional(&user_db)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    let status = state.sync_manager.account_status(&account_id).await;
    let (synced, total) = account_sync_progress(&user_db, &account_id, status.total).await?;
    Ok(Json(json!({
        "account_id": account_id,
        "state": status.state,
        "last_synced_at": status.last_synced_at,
        "error": status.error,
        "synced": synced,
        "total": total,
    })))
}

async fn account_sync_progress(
    db: &sqlx::SqlitePool,
    account_id: &str,
    fallback_total: i64,
) -> Result<(i64, i64), AppError> {
    let provider_kind: String =
        sqlx::query_scalar("SELECT provider_kind FROM email_accounts WHERE id = ?")
            .bind(account_id)
            .fetch_one(db)
            .await?;

    let synced: i64 = sqlx::query_scalar(db::queries::SYNCED_MESSAGE_COUNT_SQL)
        .bind(account_id)
        .fetch_one(db)
        .await?;

    let discovered_remote: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) \
         FROM remote_message_ids r \
         JOIN folders f ON f.account_id = r.account_id AND f.full_path = r.folder_path \
         WHERE r.account_id = ? AND f.sync_enabled = 1",
    )
    .bind(account_id)
    .fetch_one(db)
    .await?;

    let total = sync_progress_total(&provider_kind, synced, discovered_remote, fallback_total);

    Ok((synced, total))
}

fn sync_progress_total(
    provider_kind: &str,
    synced: i64,
    discovered_remote: i64,
    fallback_total: i64,
) -> i64 {
    match provider_kind {
        // API providers discover remote ids in bounded pages. Keep the
        // provider-reported denominator while a backfill is active so the GET
        // response cannot replace a larger SSE total with the smaller number
        // discovered locally so far.
        "gmail_api" | "outlook_api" => synced.max(discovered_remote).max(fallback_total),
        _ => synced.max(fallback_total),
    }
}

#[cfg(test)]
mod sync_progress_tests {
    use super::sync_progress_total;

    #[test]
    fn gmail_rest_progress_keeps_the_larger_provider_total_from_sse() {
        assert_eq!(
            sync_progress_total("gmail_api", 13_226, 12_896, 38_327),
            38_327
        );
    }

    #[test]
    fn api_progress_never_falls_below_local_or_discovered_rows() {
        assert_eq!(sync_progress_total("gmail_api", 15, 20, 10), 20);
        assert_eq!(sync_progress_total("outlook_api", 25, 20, 10), 25);
    }
}

pub async fn trigger_sync(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM email_accounts WHERE id = ?")
        .bind(&account_id)
        .fetch_optional(&user_db)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    if !state.sync_manager.force_poll(&account_id).await {
        state
            .sync_manager
            .start_account(account_id.clone(), user.0.clone(), Arc::new(state.clone()))
            .await;
        let _ = state.sync_manager.force_poll(&account_id).await;
    }

    Ok(StatusCode::ACCEPTED)
}

// ── Aliases ───────────────────────────────────────────────────────────────────

pub async fn add_alias(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
    Json(req): Json<AddAliasRequest>,
) -> Result<impl IntoResponse, AppError> {
    if !req.email.contains('@') {
        return Err(AppError::Unprocessable("invalid email format".into()));
    }
    let user_db = state.user_db_pool.get(&user.0).await?;

    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM email_accounts WHERE id = ?")
        .bind(&account_id)
        .fetch_optional(&user_db)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    let alias_id: String = sqlx::query_scalar(
        "INSERT INTO account_aliases (account_id, email, display_name) VALUES (?, ?, ?) RETURNING id",
    )
    .bind(&account_id)
    .bind(&req.email)
    .bind(req.display_name.as_deref())
    .fetch_one(&user_db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": alias_id,
            "account_id": account_id,
            "email": req.email,
            "display_name": req.display_name,
        })),
    ))
}

pub async fn list_aliases(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM email_accounts WHERE id = ?")
        .bind(&account_id)
        .fetch_optional(&user_db)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    let rows: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, email, display_name, created_at FROM account_aliases WHERE account_id = ? ORDER BY created_at",
    )
    .bind(&account_id)
    .fetch_all(&user_db)
    .await?;

    let aliases: Vec<_> = rows
        .into_iter()
        .map(|(id, email, display_name, created_at)| {
            json!({ "id": id, "account_id": account_id, "email": email, "display_name": display_name, "created_at": created_at })
        })
        .collect();

    Ok(Json(aliases))
}

pub async fn update_alias(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path((account_id, alias_id)): Path<(String, String)>,
    Json(req): Json<UpdateAliasRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM account_aliases WHERE id = ? AND account_id = ?")
            .bind(&alias_id)
            .bind(&account_id)
            .fetch_optional(&user_db)
            .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    sqlx::query("UPDATE account_aliases SET display_name = COALESCE(?, display_name) WHERE id = ?")
        .bind(req.display_name.as_deref())
        .bind(&alias_id)
        .execute(&user_db)
        .await?;

    let row: (String, String, Option<String>) =
        sqlx::query_as("SELECT id, email, display_name FROM account_aliases WHERE id = ?")
            .bind(&alias_id)
            .fetch_one(&user_db)
            .await?;

    Ok(Json(
        json!({ "id": row.0, "account_id": account_id, "email": row.1, "display_name": row.2 }),
    ))
}

pub async fn delete_alias(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path((account_id, alias_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM account_aliases WHERE id = ? AND account_id = ?")
            .bind(&alias_id)
            .bind(&account_id)
            .fetch_optional(&user_db)
            .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    sqlx::query("DELETE FROM account_aliases WHERE id = ?")
        .bind(&alias_id)
        .execute(&user_db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Persist a new sidebar order. The request must list every account exactly
/// once; sort_order becomes the position in the list.
pub async fn reorder_accounts(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<ReorderAccountsRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let existing: Vec<String> = sqlx::query_scalar("SELECT id FROM email_accounts")
        .fetch_all(&user_db)
        .await?;
    let requested: std::collections::HashSet<&str> =
        req.account_ids.iter().map(String::as_str).collect();
    if requested.len() != req.account_ids.len()
        || existing.len() != req.account_ids.len()
        || !existing.iter().all(|id| requested.contains(id.as_str()))
    {
        return Err(AppError::Unprocessable(
            "account_ids must list every account exactly once".into(),
        ));
    }

    let mut tx = user_db.begin().await?;
    for (position, id) in req.account_ids.iter().enumerate() {
        sqlx::query("UPDATE email_accounts SET sort_order = ? WHERE id = ?")
            .bind(position as i64)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── helpers ───────────────────────────────────────────────────────────────────

use std::sync::Arc;

async fn get_account_row(
    db: &sqlx::SqlitePool,
    account_id: &str,
) -> Result<AccountResponse, AppError> {
    let mut row: AccountResponse =
        sqlx::query_as(
            "SELECT id, display_name, primary_email, imap_host, imap_port, imap_auth_scheme, smtp_host, smtp_port, smtp_auth_scheme, body_sync_mode, sync_interval_secs, sync_mode, provider_kind, created_at, carddav_url, caldav_url, caldav_accept_invalid_tls, pgp_key_id, sign_by_default, color, sort_order FROM email_accounts WHERE id = ?",
        )
        .bind(account_id)
        .fetch_optional(db)
        .await?
        .ok_or(AppError::NotFound)?;
    row.contacts = contact_capability(db, account_id).await?;
    Ok(row)
}

async fn contact_capability(
    db: &sqlx::SqlitePool,
    account_id: &str,
) -> Result<Option<ContactCapabilitySummary>, AppError> {
    let row: Option<(
        String,
        String,
        String,
        Option<String>,
        bool,
        Option<String>,
        bool,
    )> = sqlx::query_as(
        "SELECT id, type, capability_state, capability_reason, enabled,
                    last_synced_at, cache_retained
             FROM contact_accounts WHERE email_account_id = ?",
    )
    .bind(account_id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(
        |(source_id, provider, state, reason, enabled, last_synced_at, cache_retained)| {
            ContactCapabilitySummary {
                source_id,
                provider,
                state,
                reason,
                enabled,
                last_synced_at,
                cache_retained,
            }
        },
    ))
}

async fn reconcile_and_start_contacts(
    state: &AppState,
    user_id: &str,
    db: &sqlx::SqlitePool,
    account_id: &str,
    enabled: Option<bool>,
) {
    match crate::contact_reconcile::reconcile_mailbox_contact_source(
        db,
        &state.credential_key,
        account_id,
        enabled,
    )
    .await
    {
        Ok(capability) if capability.enabled && capability.state == "pending" => {
            crate::routes::contacts::spawn_contact_sync_task(
                state.clone(),
                user_id.to_owned(),
                capability.source_id,
                true,
            )
            .await;
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(account_id, %error, "contact source reconciliation failed"),
    }
}
