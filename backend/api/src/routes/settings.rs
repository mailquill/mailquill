use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{error::AppError, middleware::UserId, state::AppState, validate};

#[derive(Serialize)]
pub struct UserSettingsResponse {
    pgp_discovery_wkd_enabled: bool,
    pgp_discovery_keyserver_enabled: bool,
    load_external_images: bool,
    default_calendar_id: Option<String>,
}

#[derive(Serialize)]
pub struct PublicConfigResponse {
    remote_image_proxy_enabled: bool,
    registration_enabled: bool,
}

#[derive(Deserialize)]
pub struct PatchSettingsRequest {
    pgp_discovery_wkd_enabled: Option<bool>,
    pgp_discovery_keyserver_enabled: Option<bool>,
    load_external_images: Option<bool>,
    default_calendar_id: Option<String>,
}

pub async fn get_settings(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let row: Option<(bool, bool, bool, Option<String>)> = sqlx::query_as(
        "SELECT pgp_discovery_wkd_enabled, pgp_discovery_keyserver_enabled, load_external_images, default_calendar_id FROM user_settings WHERE user_id = ?",
    )
    .bind(&user.0)
    .fetch_optional(&state.app_db)
    .await?;

    let (wkd, ks, images, default_calendar_id) = row.unwrap_or((false, false, false, None));
    Ok(Json(UserSettingsResponse {
        pgp_discovery_wkd_enabled: wkd,
        pgp_discovery_keyserver_enabled: ks,
        load_external_images: images,
        default_calendar_id,
    }))
}

pub async fn public_config(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    Ok(Json(PublicConfigResponse {
        remote_image_proxy_enabled: state.remote_image_proxy_enabled,
        registration_enabled: state.registration_enabled,
    }))
}

pub async fn patch_settings(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<PatchSettingsRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Upsert settings row
    sqlx::query(
        "INSERT INTO user_settings (user_id, pgp_discovery_wkd_enabled, pgp_discovery_keyserver_enabled) VALUES (?, 0, 0) ON CONFLICT(user_id) DO NOTHING",
    )
    .bind(&user.0)
    .execute(&state.app_db)
    .await?;

    if let Some(v) = req.pgp_discovery_wkd_enabled {
        sqlx::query("UPDATE user_settings SET pgp_discovery_wkd_enabled = ? WHERE user_id = ?")
            .bind(v)
            .bind(&user.0)
            .execute(&state.app_db)
            .await?;
    }
    if let Some(v) = req.pgp_discovery_keyserver_enabled {
        sqlx::query(
            "UPDATE user_settings SET pgp_discovery_keyserver_enabled = ? WHERE user_id = ?",
        )
        .bind(v)
        .bind(&user.0)
        .execute(&state.app_db)
        .await?;
    }
    if let Some(v) = req.load_external_images {
        sqlx::query("UPDATE user_settings SET load_external_images = ? WHERE user_id = ?")
            .bind(v)
            .bind(&user.0)
            .execute(&state.app_db)
            .await?;
    }
    if let Some(v) = req.default_calendar_id {
        let value = if v.trim().is_empty() { None } else { Some(v) };
        sqlx::query("UPDATE user_settings SET default_calendar_id = ? WHERE user_id = ?")
            .bind(value)
            .bind(&user.0)
            .execute(&state.app_db)
            .await?;
    }

    get_settings(State(state), Extension(user)).await
}

#[derive(Serialize)]
pub struct AllowedSender {
    sender: String,
    created_at: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct BrandEntry {
    id: String,
    domain: String,
    brand_name: String,
}

#[derive(Deserialize)]
pub struct AddAllowedSenderRequest {
    sender: String,
}

#[derive(Deserialize)]
pub struct AddBrandRequest {
    domain: String,
    brand_name: String,
}

pub async fn list_image_allowlist(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT sender, created_at FROM image_sender_allowlist WHERE user_id = ? ORDER BY sender ASC",
    )
    .bind(&user.0)
    .fetch_all(&state.app_db)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|(sender, created_at)| AllowedSender { sender, created_at })
            .collect::<Vec<_>>(),
    ))
}

pub async fn add_image_allowlist(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<AddAllowedSenderRequest>,
) -> Result<impl IntoResponse, AppError> {
    let sender = req.sender.trim().to_lowercase();
    validate::email("sender", &sender)?;

    sqlx::query(
        "INSERT INTO image_sender_allowlist (user_id, sender) VALUES (?, ?) ON CONFLICT(user_id, sender) DO NOTHING",
    )
    .bind(&user.0)
    .bind(&sender)
    .execute(&state.app_db)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_brands(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let rows: Vec<BrandEntry> = sqlx::query_as(
        "SELECT id, domain, brand_name FROM user_brand_entries ORDER BY brand_name COLLATE NOCASE ASC, domain ASC",
    )
    .fetch_all(&user_db)
    .await?;
    Ok(Json(rows))
}

pub async fn add_brand(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<AddBrandRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let domain = normalize_domain(&req.domain)?;
    let brand_name = req.brand_name.trim();
    if brand_name.is_empty() {
        return Err(AppError::Unprocessable("brand_name is required".into()));
    }
    let id: String = sqlx::query_scalar(
        "INSERT INTO user_brand_entries (domain, brand_name) VALUES (?, ?) \
         ON CONFLICT(domain) DO UPDATE SET brand_name = excluded.brand_name RETURNING id",
    )
    .bind(&domain)
    .bind(brand_name)
    .fetch_one(&user_db)
    .await?;
    let entry: BrandEntry =
        sqlx::query_as("SELECT id, domain, brand_name FROM user_brand_entries WHERE id = ?")
            .bind(id)
            .fetch_one(&user_db)
            .await?;
    Ok(Json(entry))
}

pub async fn delete_brand(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let rows = sqlx::query("DELETE FROM user_brand_entries WHERE id = ?")
        .bind(id)
        .execute(&user_db)
        .await?
        .rows_affected();
    if rows == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Wipe all stored phishing verdicts for the user. Each message is then
/// re-analysed lazily on next open (or at next sync for new mail) — used
/// after scorer/brands-list updates changed the rules.
pub async fn reset_phishing_analysis(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    sqlx::query("DELETE FROM phishing_analysis")
        .execute(&user_db)
        .await?;
    sqlx::query("UPDATE messages SET phishing_verdict = NULL")
        .execute(&user_db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_image_allowlist(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(sender): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let sender = sender.trim().to_lowercase();
    validate::email("sender", &sender)?;

    sqlx::query("DELETE FROM image_sender_allowlist WHERE user_id = ? AND sender = ?")
        .bind(&user.0)
        .bind(&sender)
        .execute(&state.app_db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

fn normalize_domain(value: &str) -> Result<String, AppError> {
    let trimmed = value
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("www.")
        .trim_end_matches('/')
        .to_lowercase();
    let domain = trimmed.split('/').next().unwrap_or_default();
    if domain.is_empty()
        || !domain.contains('.')
        || domain
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '.'))
    {
        return Err(AppError::Unprocessable("domain is invalid".into()));
    }
    Ok(domain.to_owned())
}
