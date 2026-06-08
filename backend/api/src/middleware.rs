use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::IntoResponse,
    Json,
};
use serde_json::json;

use crate::state::AppState;

/// Extracts and validates the JWT from the Authorization header,
/// injecting the user_id as a request extension.
pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> impl IntoResponse {
    let auth = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match auth {
        Some(token) => match state.jwt_key.validate(token) {
            Ok(user_id) => {
                req.extensions_mut().insert(UserId(user_id));
                next.run(req).await.into_response()
            }
            Err(_) => (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "unauthorized" })),
            )
                .into_response(),
        },
        None => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized" })),
        )
            .into_response(),
    }
}

/// Request extension carrying the authenticated user_id.
#[derive(Clone)]
pub struct UserId(pub String);
