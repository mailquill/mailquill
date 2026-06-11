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
}

#[derive(Deserialize)]
pub struct PatchSettingsRequest {
    pgp_discovery_wkd_enabled: Option<bool>,
    pgp_discovery_keyserver_enabled: Option<bool>,
    load_external_images: Option<bool>,
}

pub async fn get_settings(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let row: Option<(bool, bool, bool)> = sqlx::query_as(
        "SELECT pgp_discovery_wkd_enabled, pgp_discovery_keyserver_enabled, load_external_images FROM user_settings WHERE user_id = ?",
    )
    .bind(&user.0)
    .fetch_optional(&state.app_db)
    .await?;

    let (wkd, ks, images) = row.unwrap_or((false, false, false));
    Ok(Json(UserSettingsResponse {
        pgp_discovery_wkd_enabled: wkd,
        pgp_discovery_keyserver_enabled: ks,
        load_external_images: images,
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

    get_settings(State(state), Extension(user)).await
}

#[derive(Serialize)]
pub struct AllowedSender {
    sender: String,
    created_at: String,
}

#[derive(Deserialize)]
pub struct AddAllowedSenderRequest {
    sender: String,
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

/// Wipe all stored phishing verdicts for the user. Each message is then
/// re-analysed lazily on next open (or at next sync for new mail) — used
/// after scorer/brands-list updates changed the rules.
pub async fn reset_phishing_analysis(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    sqlx::query("DELETE FROM phishing_analysis").execute(&user_db).await?;
    sqlx::query("UPDATE messages SET phishing_verdict = NULL").execute(&user_db).await?;
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
