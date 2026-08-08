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

#[derive(Deserialize)]
pub struct LoginRequest {
    email: String,
    password: String,
    /// "Remember me": issues a long-lived, browser-persistent refresh cookie
    /// instead of a session-scoped one. Defaults to false so an unchecked box
    /// doesn't outlive the browser session.
    #[serde(default)]
    remember: bool,
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
    if !state.registration_enabled {
        return Err(AppError::Forbidden);
    }
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

    // No "remember me" checkbox at registration — a brand new account starts
    // out remembered, same as before this option existed.
    let (_rt, cookie) = issue_refresh_token(&state, &user_id, true).await?;

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
    Json(req): Json<LoginRequest>,
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

    let (_rt, cookie) = issue_refresh_token(&state, &user_id, req.remember).await?;

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

    let row: Option<(String, String, bool, bool, bool)> = sqlx::query_as(
        "SELECT id, user_id, (revoked = 1), (replaced_at IS NOT NULL AND replaced_at > datetime('now', '-60 seconds')), remember FROM refresh_tokens WHERE token_hash = ? AND expires_at > datetime('now')",
    )
    .bind(&token_hash)
    .fetch_optional(&state.app_db)
    .await?;

    let (token_id, user_id, revoked, recently_replaced, remember) =
        row.ok_or(AppError::Unauthorized)?;

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

    // Sliding window: rotation carries the original login's remember choice
    // forward, so a remembered session keeps renewing at the long TTL for as
    // long as the user stays active, and an unremembered one keeps renewing
    // at the short one.
    let (_rt, cookie) = issue_refresh_token(&state, &user_id, remember).await?;

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

/// Sliding-window lifetime for a "remember me" session. Comfortably above the
/// 7-day minimum so an active user is never forced to re-authenticate.
const REMEMBER_TTL: chrono::Duration = chrono::Duration::days(30);
/// Lifetime for a session that wasn't remembered. The cookie itself is
/// browser-session-scoped (no Max-Age) and disappears on browser close; this
/// is a server-side backstop for browsers that restore session cookies
/// across restarts.
const SESSION_TTL: chrono::Duration = chrono::Duration::hours(24);

async fn issue_refresh_token(
    state: &AppState,
    user_id: &str,
    remember: bool,
) -> Result<(String, String), AppError> {
    let token = generate_token();
    let token_hash = hash_token(&token);
    let ttl = if remember { REMEMBER_TTL } else { SESSION_TTL };
    let expires_at = (Utc::now() + ttl).to_rfc3339();

    sqlx::query(
        "INSERT INTO refresh_tokens (user_id, token_hash, expires_at, remember) VALUES (?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(&token_hash)
    .bind(&expires_at)
    .bind(remember)
    .execute(&state.app_db)
    .await?;

    let cookie = refresh_cookie(&token, remember);
    Ok((token, cookie))
}

/// Build the `Set-Cookie` value for a refresh token. Remembered sessions get
/// an explicit `Max-Age` so the cookie survives a browser restart;
/// unremembered ones omit it so the browser drops the cookie on close.
fn refresh_cookie(token: &str, remember: bool) -> String {
    let base = format!("refresh_token={token}; HttpOnly; SameSite=Strict; Path=/api/auth/refresh");
    if remember {
        format!("{base}; Max-Age={}", REMEMBER_TTL.num_seconds())
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembered_sessions_get_a_persistent_min_seven_day_max_age() {
        let cookie = refresh_cookie("tok", true);
        assert!(cookie.contains("Max-Age=2592000"));
        assert!(REMEMBER_TTL >= chrono::Duration::days(7));
    }

    #[test]
    fn unremembered_sessions_get_a_browser_session_cookie() {
        let cookie = refresh_cookie("tok", false);
        assert!(!cookie.contains("Max-Age"));
        assert!(cookie.starts_with("refresh_token=tok;"));
    }
}
