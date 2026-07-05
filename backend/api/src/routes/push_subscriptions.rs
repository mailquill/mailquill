use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{error::AppError, middleware::UserId, state::AppState};

#[derive(Serialize)]
pub struct VapidPublicKeyResponse {
    public_key: String,
}

#[derive(Deserialize)]
pub struct PushSubscriptionRequest {
    endpoint: String,
    keys: PushSubscriptionKeys,
}

#[derive(Deserialize)]
pub struct PushSubscriptionKeys {
    p256dh: String,
    auth: String,
}

#[derive(Serialize)]
pub struct PushSubscriptionResponse {
    id: String,
}

pub async fn vapid_public_key(State(state): State<AppState>) -> impl IntoResponse {
    // Web push is optional: when VAPID isn't configured, return an empty key
    // (HTTP 200) instead of an error. The client treats a missing key as
    // "push unavailable" and disables the toggle — no need to log a 500.
    Json(VapidPublicKeyResponse {
        public_key: state
            .vapid
            .as_ref()
            .map(|v| v.public_key.clone())
            .unwrap_or_default(),
    })
}

pub async fn create_subscription(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<PushSubscriptionRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.endpoint.trim().is_empty()
        || req.keys.p256dh.trim().is_empty()
        || req.keys.auth.trim().is_empty()
    {
        return Err(AppError::Unprocessable(
            "push subscription endpoint and keys are required".to_owned(),
        ));
    }

    let id: String = sqlx::query_scalar(
        "INSERT INTO push_subscriptions (user_id, endpoint, p256dh, auth) VALUES (?, ?, ?, ?) \
         ON CONFLICT(user_id, endpoint) DO UPDATE SET p256dh = excluded.p256dh, auth = excluded.auth \
         RETURNING id",
    )
    .bind(&user.0)
    .bind(req.endpoint)
    .bind(req.keys.p256dh)
    .bind(req.keys.auth)
    .fetch_one(&state.app_db)
    .await?;

    Ok(Json(PushSubscriptionResponse { id }))
}

pub async fn delete_subscription(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    sqlx::query("DELETE FROM push_subscriptions WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(&user.0)
        .execute(&state.app_db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_stale_subscription(state: &AppState, id: &str) {
    if let Err(e) = sqlx::query("DELETE FROM push_subscriptions WHERE id = ?")
        .bind(id)
        .execute(&state.app_db)
        .await
    {
        tracing::warn!("failed to delete stale push subscription: {e}");
    }
}

pub fn push_payload(
    message_id: &str,
    account_id: &str,
    account_name: &str,
    sender: &str,
    subject: &str,
) -> Result<Vec<u8>, AppError> {
    serde_json::to_vec(&json!({
        "title": sender,
        "body": subject,
        "account_name": account_name,
        "message_url": format!("/mail/{account_id}/inbox/{message_id}"),
    }))
    .map_err(|e| AppError::Internal(e.to_string()))
}
