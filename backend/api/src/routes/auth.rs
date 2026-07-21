use argon2::{
    password_hash::{PasswordHash, PasswordVerifier},
    Argon2,
};
use axum::{
    extract::{Extension, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use mailquill_core::crypto::{generate_token, hash_token};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    error::AppError,
    middleware::UserId,
    passwords::{hash_password, PasswordResetError},
    state::AppState,
};

#[derive(Deserialize)]
pub struct RegisterRequest {
    email: String,
    password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    access_token: String,
    token_type: String,
}

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AppError> {
    if !req.email.contains('@') {
        return Err(AppError::Unprocessable("invalid email address".into()));
    }

    let email = req.email.to_lowercase();
    let hash = hash_password(&req.password).map_err(|error| match error {
        PasswordResetError::TooShort => AppError::Unprocessable(error.to_string()),
        _ => AppError::Internal(error.to_string()),
    })?;

    let user_id: String =
        sqlx::query_scalar("INSERT INTO users (email, password_hash) VALUES (?, ?) RETURNING id")
            .bind(&email)
            .bind(&hash)
            .fetch_one(&state.app_db)
            .await
            .map_err(|e| match e {
                sqlx::Error::Database(ref db) if db.is_unique_violation() => {
                    AppError::Conflict("email already registered".into())
                }
                other => AppError::Internal(other.to_string()),
            })?;

    // Insert user settings with privacy defaults off (D9)
    sqlx::query(
        "INSERT OR IGNORE INTO user_settings (user_id, pgp_discovery_wkd_enabled, pgp_discovery_keyserver_enabled) VALUES (?, 0, 0)",
    )
    .bind(&user_id)
    .execute(&state.app_db)
    .await?;

    let access_token = state
        .jwt_key
        .issue_access_token(&user_id)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let (_rt, cookie) = issue_refresh_token(&state, &user_id).await?;

    let mut headers = HeaderMap::new();
    headers.insert("Set-Cookie", cookie.parse().unwrap());
    Ok((
        StatusCode::CREATED,
        headers,
        Json(AuthResponse {
            access_token,
            token_type: "Bearer".into(),
        }),
    ))
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AppError> {
    let email = req.email.to_lowercase();

    let row: Option<(String, String)> =
        sqlx::query_as("SELECT id, password_hash FROM users WHERE email = ?")
            .bind(&email)
            .fetch_optional(&state.app_db)
            .await?;

    let (user_id, hash) = row.ok_or(AppError::Unauthorized)?;

    let parsed = PasswordHash::new(&hash).map_err(|e| AppError::Internal(e.to_string()))?;
    Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed)
        .map_err(|_| AppError::Unauthorized)?;

    let access_token = state
        .jwt_key
        .issue_access_token(&user_id)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let (_rt, cookie) = issue_refresh_token(&state, &user_id).await?;

    let mut headers = HeaderMap::new();
    headers.insert("Set-Cookie", cookie.parse().unwrap());
    Ok((
        headers,
        Json(AuthResponse {
            access_token,
            token_type: "Bearer".into(),
        }),
    ))
}

pub async fn refresh(
    State(state): State<AppState>,
    req: Request,
) -> Result<impl IntoResponse, AppError> {
    let raw = cookie_from_request(&req, "refresh_token").ok_or(AppError::Unauthorized)?;
    let token_hash = hash_token(&raw);

    let row: Option<(String, String, bool, bool)> = sqlx::query_as(
        "SELECT id, user_id, (revoked = 1), (replaced_at IS NOT NULL AND replaced_at > datetime('now', '-60 seconds')) FROM refresh_tokens WHERE token_hash = ? AND expires_at > datetime('now')",
    )
    .bind(&token_hash)
    .fetch_optional(&state.app_db)
    .await?;

    let (token_id, user_id, revoked, recently_replaced) = row.ok_or(AppError::Unauthorized)?;

    if revoked {
        // Reuse grace: a token rotated moments ago is a benign concurrent or
        // multi-tab refresh (the client raced its own parallel requests, or a
        // second tab is open). Issue a fresh token for this client instead of
        // logging it out. Outside the grace window — or for a token revoked by
        // logout (replaced_at stays NULL) — this is genuine reuse: reject.
        if !recently_replaced {
            return Err(AppError::Unauthorized);
        }
    } else {
        // Normal rotation: mark the presented token replaced.
        sqlx::query(
            "UPDATE refresh_tokens SET revoked = 1, replaced_at = datetime('now') WHERE id = ?",
        )
        .bind(&token_id)
        .execute(&state.app_db)
        .await?;
    }

    let access_token = state
        .jwt_key
        .issue_access_token(&user_id)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let (_rt, cookie) = issue_refresh_token(&state, &user_id).await?;

    let mut headers = HeaderMap::new();
    headers.insert("Set-Cookie", cookie.parse().unwrap());
    Ok((
        headers,
        Json(AuthResponse {
            access_token,
            token_type: "Bearer".into(),
        }),
    ))
}

pub async fn logout(
    State(state): State<AppState>,
    req: Request,
) -> Result<impl IntoResponse, AppError> {
    if let Some(raw) = cookie_from_request(&req, "refresh_token") {
        let token_hash = hash_token(&raw);
        sqlx::query("UPDATE refresh_tokens SET revoked = 1 WHERE token_hash = ?")
            .bind(&token_hash)
            .execute(&state.app_db)
            .await?;
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        "Set-Cookie",
        HeaderValue::from_static(
            "refresh_token=; HttpOnly; SameSite=Strict; Path=/api/auth/refresh; Max-Age=0",
        ),
    );
    Ok((headers, Json(json!({ "ok": true }))))
}

pub async fn me(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let row: Option<(String, String)> = sqlx::query_as("SELECT id, email FROM users WHERE id = ?")
        .bind(&user.0)
        .fetch_optional(&state.app_db)
        .await?;

    let (id, email) = row.ok_or(AppError::NotFound)?;
    Ok(Json(json!({ "id": id, "email": email })))
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn cookie_from_request(req: &Request, name: &str) -> Option<String> {
    req.headers()
        .get("Cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|raw| {
            raw.split(';').find_map(|pair| {
                let pair = pair.trim();
                let (k, v) = pair.split_once('=')?;
                if k.trim() == name {
                    Some(v.trim().to_owned())
                } else {
                    None
                }
            })
        })
}

async fn issue_refresh_token(
    state: &AppState,
    user_id: &str,
) -> Result<(String, String), AppError> {
    let token = generate_token();
    let token_hash = hash_token(&token);
    let expires_at = (Utc::now() + chrono::Duration::days(30)).to_rfc3339();

    sqlx::query("INSERT INTO refresh_tokens (user_id, token_hash, expires_at) VALUES (?, ?, ?)")
        .bind(user_id)
        .bind(&token_hash)
        .bind(&expires_at)
        .execute(&state.app_db)
        .await?;

    let cookie = format!(
        "refresh_token={}; HttpOnly; SameSite=Strict; Path=/api/auth/refresh; Max-Age=2592000",
        token
    );
    Ok((token, cookie))
}
