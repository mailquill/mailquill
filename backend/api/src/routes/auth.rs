//! Local account authentication: registration, password login, refresh-token
//! rotation (HttpOnly cookie) and logout.

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
    rate_limit::{forwarded_client_ip, LOGIN_MAX_FAILURES, REGISTER_MAX_ATTEMPTS},
    state::AppState,
};

/// Body of `POST /api/auth/register`.
#[derive(Deserialize)]
pub struct RegisterRequest {
    email: String,
    password: String,
}

/// Body of `POST /api/auth/login`.
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

/// Access token returned by register, login and refresh; the refresh token
/// travels in a `Set-Cookie` header instead.
#[derive(Serialize)]
pub struct AuthResponse {
    access_token: String,
    token_type: String,
}

/// `POST /api/auth/register`: create a local account.
///
/// Rejected with 403 when registration is disabled, 429 when the forwarded
/// client address exceeded its sign-up budget, 422 for a malformed email or
/// too-short password and 409 for an already registered email. On success
/// returns 201 with an access token and a remembered refresh cookie.
pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AppError> {
    if !state.registration_enabled {
        return Err(AppError::Forbidden);
    }
    // Throttle mass sign-ups from one client when a proxy reports its address.
    if let Some(ip) = forwarded_client_ip(&headers) {
        let key = format!("register-ip:{ip}");
        if state.login_limiter.is_blocked(&key, REGISTER_MAX_ATTEMPTS) {
            return Err(AppError::TooManyRequests);
        }
        state.login_limiter.record_failure(&key);
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

/// `POST /api/auth/login`: verify email and password.
///
/// Blocked (429) once the account or client address reached the failure
/// budget. Unknown accounts are verified against a dummy hash so timing does
/// not reveal registered addresses. Returns an access token plus a refresh
/// cookie whose lifetime follows the `remember` flag; failures count against
/// the throttle and yield 401.
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    let email = req.email.to_lowercase();

    // Brute-force throttle: per account, and per client address when a
    // reverse proxy reports one. Checked before any work so a locked key costs
    // the attacker nothing but a 429.
    let mut limit_keys = vec![format!("email:{email}")];
    if let Some(ip) = forwarded_client_ip(&headers) {
        limit_keys.push(format!("ip:{ip}"));
    }
    if limit_keys
        .iter()
        .any(|key| state.login_limiter.is_blocked(key, LOGIN_MAX_FAILURES))
    {
        return Err(AppError::TooManyRequests);
    }

    let row: Option<(String, String)> =
        sqlx::query_as("SELECT id, password_hash FROM users WHERE email = ?")
            .bind(&email)
            .fetch_optional(&state.app_db)
            .await?;

    // Verify against a dummy hash when the account does not exist, so the
    // response time does not reveal which addresses are registered.
    let (user_id, hash) = match row {
        Some(row) => row,
        None => (String::new(), dummy_password_hash().to_owned()),
    };
    let parsed = PasswordHash::new(&hash).map_err(|e| AppError::Internal(e.to_string()))?;
    let verified = Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed)
        .is_ok();
    if !verified || user_id.is_empty() {
        for key in &limit_keys {
            state.login_limiter.record_failure(key);
        }
        return Err(AppError::Unauthorized);
    }
    state.login_limiter.clear(&limit_keys[0]);

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

/// `POST /api/auth/refresh`: rotate the refresh cookie and mint a new access token.
///
/// The presented token must exist, be unexpired and either unrevoked or
/// replaced within the last 60 seconds (concurrent-tab grace); any other
/// revoked token is treated as reuse and rejected with 401. The new token
/// keeps the original session's `remember` choice.
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

/// `POST /api/auth/logout`: revoke the presented refresh token (if any) and
/// clear the cookie. Always succeeds.
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
    let mut clear = String::from(
        "refresh_token=; HttpOnly; SameSite=Strict; Path=/api/auth/refresh; Max-Age=0",
    );
    if state.secure_cookies {
        clear.push_str("; Secure");
    }
    headers.insert(
        "Set-Cookie",
        HeaderValue::from_str(&clear).unwrap_or_else(|_| {
            HeaderValue::from_static(
                "refresh_token=; HttpOnly; SameSite=Strict; Path=/api/auth/refresh; Max-Age=0",
            )
        }),
    );
    Ok((headers, Json(json!({ "ok": true }))))
}

/// A valid Argon2 hash of a throwaway password, verified against when a login
/// names an unknown account so that path takes as long as a real check.
fn dummy_password_hash() -> &'static str {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        hash_password("mailquill-dummy-password-for-timing")
            .expect("hashing a fixed valid password cannot fail")
    })
}

/// `GET /api/auth/me`: id and email of the authenticated user, 404 if the
/// user row no longer exists.
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

    let cookie = refresh_cookie(&token, remember, state.secure_cookies);
    Ok((token, cookie))
}

/// Build the `Set-Cookie` value for a refresh token. Remembered sessions get
/// an explicit `Max-Age` so the cookie survives a browser restart;
/// unremembered ones omit it so the browser drops the cookie on close.
/// `secure` marks the cookie https-only; it follows the configured base URL
/// so a plain-http development setup keeps working.
fn refresh_cookie(token: &str, remember: bool, secure: bool) -> String {
    let mut cookie =
        format!("refresh_token={token}; HttpOnly; SameSite=Strict; Path=/api/auth/refresh");
    if remember {
        cookie.push_str(&format!("; Max-Age={}", REMEMBER_TTL.num_seconds()));
    }
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembered_sessions_get_a_persistent_min_seven_day_max_age() {
        let cookie = refresh_cookie("tok", true, false);
        assert!(cookie.contains("Max-Age=2592000"));
        assert!(REMEMBER_TTL >= chrono::Duration::days(7));
    }

    #[test]
    fn unremembered_sessions_get_a_browser_session_cookie() {
        let cookie = refresh_cookie("tok", false, false);
        assert!(!cookie.contains("Max-Age"));
        assert!(cookie.starts_with("refresh_token=tok;"));
    }

    #[test]
    fn https_deployments_get_secure_cookies() {
        assert!(refresh_cookie("tok", true, true).ends_with("; Secure"));
        assert!(!refresh_cookie("tok", true, false).contains("Secure"));
    }

    #[test]
    fn dummy_hash_verifies_only_its_own_password() {
        let parsed = PasswordHash::new(dummy_password_hash()).unwrap();
        assert!(Argon2::default()
            .verify_password(b"wrong", &parsed)
            .is_err());
    }
}
