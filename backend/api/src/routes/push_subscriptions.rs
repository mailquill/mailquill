//! Web Push subscriptions: VAPID key exposure, subscription storage and the
//! notification payload sent by the sync manager.

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{error::AppError, middleware::UserId, state::AppState};

/// Public VAPID key for `PushManager.subscribe`; empty when push is not configured.
#[derive(Serialize)]
pub struct VapidPublicKeyResponse {
    public_key: String,
}

/// Body of `POST /api/push-subscriptions`: a browser `PushSubscription`.
#[derive(Deserialize)]
pub struct PushSubscriptionRequest {
    endpoint: String,
    keys: PushSubscriptionKeys,
}

/// Client keys of a push subscription.
#[derive(Deserialize)]
pub struct PushSubscriptionKeys {
    p256dh: String,
    auth: String,
}

/// Id of a stored subscription.
#[derive(Serialize)]
pub struct PushSubscriptionResponse {
    id: String,
}

/// `GET /api/push-subscriptions/vapid-public-key`: the server's VAPID public
/// key, or an empty string (still 200) when web push is disabled.
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

/// `POST /api/push-subscriptions`: store or refresh a subscription for the
/// user (unique per endpoint). Endpoint and keys must be non-empty (422).
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

/// `DELETE /api/push-subscriptions/{id}`: remove the user's subscription; 204
/// even if it did not exist.
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

/// Drop a subscription the push service rejected (gone or invalid). Failures
/// are only logged.
pub async fn delete_stale_subscription(state: &AppState, id: &str) {
    if let Err(e) = sqlx::query("DELETE FROM push_subscriptions WHERE id = ?")
        .bind(id)
        .execute(&state.app_db)
        .await
    {
        tracing::warn!("failed to delete stale push subscription: {e}");
    }
}

/// Serialise the notification payload for a new message: title (sender), body
/// (subject), account name and the in-app URL of the message.
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
