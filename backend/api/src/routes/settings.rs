use axum::{
    extract::{Extension, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{error::AppError, middleware::UserId, state::AppState};

#[derive(Serialize)]
pub struct UserSettingsResponse {
    pgp_discovery_wkd_enabled: bool,
    pgp_discovery_keyserver_enabled: bool,
}

#[derive(Deserialize)]
pub struct PatchSettingsRequest {
    pgp_discovery_wkd_enabled: Option<bool>,
    pgp_discovery_keyserver_enabled: Option<bool>,
}

pub async fn get_settings(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let row: Option<(bool, bool)> = sqlx::query_as(
        "SELECT pgp_discovery_wkd_enabled, pgp_discovery_keyserver_enabled FROM user_settings WHERE user_id = ?",
    )
    .bind(&user.0)
    .fetch_optional(&state.app_db)
    .await?;

    let (wkd, ks) = row.unwrap_or((false, false));
    Ok(Json(UserSettingsResponse {
        pgp_discovery_wkd_enabled: wkd,
        pgp_discovery_keyserver_enabled: ks,
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

    get_settings(State(state), Extension(user)).await
}
