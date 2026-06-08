use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{error::AppError, middleware::UserId, state::AppState};

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
}

#[derive(Serialize)]
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
    created_at: String,
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
    let body_sync_mode = req.body_sync_mode.as_deref().unwrap_or("lazy");
    if !matches!(body_sync_mode, "lazy" | "full") {
        return Err(AppError::Unprocessable("body_sync_mode must be 'lazy' or 'full'".into()));
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
    let auth_scheme = req.imap_auth_scheme.clone().unwrap_or_else(|| "plain".into());

    imap_sync::test_imap_connection(&imap_host, imap_port, &imap_user, &imap_pass, &auth_scheme)
        .await
        .map_err(|e| AppError::Unprocessable(e.to_string()))?;

    let user_db = state.user_db_pool.get(&user.0).await?;
    let account_id: String = sqlx::query_scalar(
        "INSERT INTO email_accounts (display_name, primary_email, imap_host, imap_port, imap_auth_scheme, smtp_host, smtp_port, smtp_auth_scheme, credentials_encrypted, body_sync_mode, sync_interval_secs) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
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
    .fetch_one(&user_db)
    .await?;

    // Kick off initial sync
    state.sync_manager.start_account(
        account_id.clone(),
        user.0.clone(),
        Arc::new(state.clone()),
    ).await;

    let row = get_account_row(&user_db, &account_id).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn list_accounts(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let rows: Vec<AccountResponse> = sqlx::query_as(
        "SELECT id, display_name, primary_email, imap_host, imap_port, imap_auth_scheme, smtp_host, smtp_port, smtp_auth_scheme, body_sync_mode, sync_interval_secs, created_at FROM email_accounts ORDER BY created_at",
    )
    .fetch_all(&user_db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
    .into_iter()
    .map(|(id, display_name, primary_email, imap_host, imap_port, imap_auth_scheme, smtp_host, smtp_port, smtp_auth_scheme, body_sync_mode, sync_interval_secs, created_at): (String, String, String, String, i64, String, String, i64, String, String, i64, String)| AccountResponse {
        id, display_name, primary_email, imap_host, imap_port, imap_auth_scheme, smtp_host, smtp_port, smtp_auth_scheme, body_sync_mode, sync_interval_secs, created_at,
    })
    .collect();

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
    let user_db = state.user_db_pool.get(&user.0).await?;

    // Check ownership (returns 404 for both missing and wrong owner — D2)
    let existing: Option<(String, Vec<u8>)> = sqlx::query_as(
        "SELECT id, credentials_encrypted FROM email_accounts WHERE id = ?",
    )
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

    if let Some(u) = &req.imap_username { creds["imap_username"] = json!(u); }
    if let Some(p) = &req.imap_password { creds["imap_password"] = json!(p); }
    if let Some(u) = &req.smtp_username { creds["smtp_username"] = json!(u); }
    if let Some(p) = &req.smtp_password { creds["smtp_password"] = json!(p); }

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
            let scheme = req.imap_auth_scheme.clone().unwrap_or_else(|| "plain".into());
            imap_sync::test_imap_connection(host, port, &imap_user, &imap_pass, &scheme)
                .await
                .map_err(|e| AppError::Unprocessable(e.to_string()))?;
        }
    }

    sqlx::query(
        "UPDATE email_accounts SET display_name = COALESCE(?, display_name), imap_host = COALESCE(?, imap_host), imap_port = COALESCE(?, imap_port), imap_auth_scheme = COALESCE(?, imap_auth_scheme), smtp_host = COALESCE(?, smtp_host), smtp_port = COALESCE(?, smtp_port), smtp_auth_scheme = COALESCE(?, smtp_auth_scheme), credentials_encrypted = ?, body_sync_mode = COALESCE(?, body_sync_mode), sync_interval_secs = COALESCE(?, sync_interval_secs) WHERE id = ?",
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
    .bind(&account_id)
    .execute(&user_db)
    .await?;

    let row = get_account_row(&user_db, &account_id).await?;
    Ok(Json(row))
}

pub async fn delete_account(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM email_accounts WHERE id = ?")
            .bind(&account_id)
            .fetch_optional(&user_db)
            .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    // Cancel sync task before deleting DB rows
    state.sync_manager.stop_account(&account_id).await;

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

    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM email_accounts WHERE id = ?")
            .bind(&account_id)
            .fetch_optional(&user_db)
            .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    let status = state.sync_manager.account_status(&account_id).await;
    Ok(Json(json!({
        "account_id": account_id,
        "state": status.state,
        "last_synced_at": status.last_synced_at,
        "error": status.error,
    })))
}

pub async fn trigger_sync(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM email_accounts WHERE id = ?")
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

    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM email_accounts WHERE id = ?")
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

    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM email_accounts WHERE id = ?")
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

    let exists: Option<String> = sqlx::query_scalar(
        "SELECT id FROM account_aliases WHERE id = ? AND account_id = ?",
    )
    .bind(&alias_id)
    .bind(&account_id)
    .fetch_optional(&user_db)
    .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    sqlx::query(
        "UPDATE account_aliases SET display_name = COALESCE(?, display_name) WHERE id = ?",
    )
    .bind(req.display_name.as_deref())
    .bind(&alias_id)
    .execute(&user_db)
    .await?;

    let row: (String, String, Option<String>) = sqlx::query_as(
        "SELECT id, email, display_name FROM account_aliases WHERE id = ?",
    )
    .bind(&alias_id)
    .fetch_one(&user_db)
    .await?;

    Ok(Json(json!({ "id": row.0, "account_id": account_id, "email": row.1, "display_name": row.2 })))
}

pub async fn delete_alias(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path((account_id, alias_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    let exists: Option<String> = sqlx::query_scalar(
        "SELECT id FROM account_aliases WHERE id = ? AND account_id = ?",
    )
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

// ── helpers ───────────────────────────────────────────────────────────────────

use std::sync::Arc;

async fn get_account_row(
    db: &sqlx::SqlitePool,
    account_id: &str,
) -> Result<AccountResponse, AppError> {
    let row: Option<(String, String, String, String, i64, String, String, i64, String, String, i64, String)> =
        sqlx::query_as(
            "SELECT id, display_name, primary_email, imap_host, imap_port, imap_auth_scheme, smtp_host, smtp_port, smtp_auth_scheme, body_sync_mode, sync_interval_secs, created_at FROM email_accounts WHERE id = ?",
        )
        .bind(account_id)
        .fetch_optional(db)
        .await?;

    let (id, display_name, primary_email, imap_host, imap_port, imap_auth_scheme, smtp_host, smtp_port, smtp_auth_scheme, body_sync_mode, sync_interval_secs, created_at) =
        row.ok_or(AppError::NotFound)?;

    Ok(AccountResponse {
        id,
        display_name,
        primary_email,
        imap_host,
        imap_port,
        imap_auth_scheme,
        smtp_host,
        smtp_port,
        smtp_auth_scheme,
        body_sync_mode,
        sync_interval_secs,
        created_at,
    })
}
