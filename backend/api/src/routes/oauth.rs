use axum::{
    extract::{Extension, Path, Query, State},
    response::{IntoResponse, Redirect},
};
use oauth2::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope, TokenResponse,
};
use serde::Deserialize;
use std::collections::HashMap;
use tokio::sync::Mutex;

use crate::{error::AppError, middleware::UserId, state::AppState};

// Providers
const PROVIDER_GOOGLE: &str = "google";
const PROVIDER_MICROSOFT: &str = "microsoft";

#[derive(Deserialize)]
pub struct OAuthCallbackQuery {
    code: String,
    state: String,
}

/// Temporary PKCE verifier store (in-memory, keyed by CSRF state token).
/// Production use: persist in Redis or app DB.
pub type PkceStore = Mutex<HashMap<String, (String, PkceCodeVerifier, String)>>;

pub fn new_pkce_store() -> PkceStore {
    Mutex::new(HashMap::new())
}

fn provider_config(
    provider: &str,
) -> Result<(oauth2::AuthUrl, oauth2::TokenUrl), AppError> {
    match provider {
        PROVIDER_GOOGLE => Ok((
            oauth2::AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".into())
                .map_err(|e| AppError::Internal(e.to_string()))?,
            oauth2::TokenUrl::new("https://oauth2.googleapis.com/token".into())
                .map_err(|e| AppError::Internal(e.to_string()))?,
        )),
        PROVIDER_MICROSOFT => Ok((
            oauth2::AuthUrl::new(
                "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
            )
            .map_err(|e| AppError::Internal(e.to_string()))?,
            oauth2::TokenUrl::new(
                "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
            )
            .map_err(|e| AppError::Internal(e.to_string()))?,
        )),
        _ => Err(AppError::NotFound),
    }
}

fn provider_scopes(provider: &str) -> Vec<Scope> {
    match provider {
        PROVIDER_GOOGLE => vec![
            Scope::new("https://mail.google.com/".into()),
            Scope::new("email".into()),
            Scope::new("profile".into()),
        ],
        PROVIDER_MICROSOFT => vec![
            Scope::new("https://outlook.office.com/IMAP.AccessAsUser.All".into()),
            Scope::new("https://outlook.office.com/SMTP.Send".into()),
            Scope::new("offline_access".into()),
            Scope::new("email".into()),
        ],
        _ => vec![],
    }
}

fn build_client(
    provider: &str,
    state: &AppState,
) -> Result<oauth2::basic::BasicClient, AppError> {
    let (auth_url, token_url) = provider_config(provider)?;
    let env_prefix = match provider {
        PROVIDER_GOOGLE => "GOOGLE",
        PROVIDER_MICROSOFT => "MICROSOFT",
        _ => return Err(AppError::NotFound),
    };
    let client_id = std::env::var(format!("{env_prefix}_OAUTH_CLIENT_ID"))
        .unwrap_or_else(|_| "placeholder".into());
    let client_secret = std::env::var(format!("{env_prefix}_OAUTH_CLIENT_SECRET"))
        .unwrap_or_else(|_| "placeholder".into());
    let redirect_base = std::env::var("APP_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".into());
    let redirect_url = RedirectUrl::new(format!(
        "{redirect_base}/api/auth/oauth/{provider}/callback"
    ))
    .map_err(|e| AppError::Internal(e.to_string()))?;
    let _ = state; // state not needed here but kept for extensibility

    Ok(oauth2::basic::BasicClient::new(
        ClientId::new(client_id),
        Some(ClientSecret::new(client_secret)),
        auth_url,
        Some(token_url),
    )
    .set_redirect_uri(redirect_url))
}

pub async fn oauth_start(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(provider): Path<String>,
    axum::extract::Extension(pkce_store): axum::extract::Extension<std::sync::Arc<PkceStore>>,
) -> Result<impl IntoResponse, AppError> {
    let client = build_client(&provider, &state)?;
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let scopes = provider_scopes(&provider);
    let mut builder = client.authorize_url(CsrfToken::new_random);
    for scope in scopes {
        builder = builder.add_scope(scope);
    }
    let (auth_url, csrf_token) = builder.set_pkce_challenge(pkce_challenge).url();

    let state_val = csrf_token.secret().clone();
    pkce_store.lock().await.insert(
        state_val.clone(),
        (provider.clone(), pkce_verifier, user.0.clone()),
    );

    Ok(Redirect::temporary(auth_url.as_str()))
}

pub async fn oauth_callback(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(params): Query<OAuthCallbackQuery>,
    axum::extract::Extension(pkce_store): axum::extract::Extension<std::sync::Arc<PkceStore>>,
) -> Result<impl IntoResponse, AppError> {
    let (stored_provider, pkce_verifier, user_id) = pkce_store
        .lock()
        .await
        .remove(&params.state)
        .ok_or(AppError::Unauthorized)?;

    if stored_provider != provider {
        return Err(AppError::Unauthorized);
    }

    let client = build_client(&provider, &state)?;
    let token_result = client
        .exchange_code(AuthorizationCode::new(params.code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(oauth2::reqwest::async_http_client)
        .await
        .map_err(|e| AppError::BadGateway(format!("oauth token exchange: {e}")))?;

    let refresh_token = token_result
        .refresh_token()
        .map(|t| t.secret().to_owned())
        .unwrap_or_default();
    let access_token = token_result.access_token().secret().to_owned();

    // Encrypt and store OAuth refresh token
    let creds = serde_json::json!({
        "oauth_access_token": access_token,
        "oauth_refresh_token": refresh_token,
        "provider": provider,
    });
    let creds_bytes = serde_json::to_vec(&creds).unwrap();
    let encrypted = state
        .credential_key
        .encrypt(&creds_bytes)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Upsert the OAuth credentials for this user's account (email from token introspection not done here;
    // the frontend will pass the email when creating the account via POST /accounts with auth_scheme=xoauth2)
    let user_db = state.user_db_pool.get(&user_id).await?;
    // Email must be provided by user when setting up account via POST /accounts
    let account_email = "oauth@pending".to_owned();

    let imap_host = match provider.as_str() {
        PROVIDER_GOOGLE => "imap.gmail.com",
        PROVIDER_MICROSOFT => "outlook.office365.com",
        _ => return Err(AppError::NotFound),
    };
    let smtp_host = match provider.as_str() {
        PROVIDER_GOOGLE => "smtp.gmail.com",
        PROVIDER_MICROSOFT => "smtp.office365.com",
        _ => return Err(AppError::NotFound),
    };

    let account_id: String = sqlx::query_scalar(
        "INSERT INTO email_accounts (display_name, primary_email, imap_host, imap_port, imap_auth_scheme, smtp_host, smtp_port, smtp_auth_scheme, credentials_encrypted, body_sync_mode) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(format!("{} ({})", if provider == PROVIDER_GOOGLE { "Gmail" } else { "Outlook" }, &account_email))
    .bind(&account_email)
    .bind(imap_host)
    .bind(993i64)
    .bind("xoauth2")
    .bind(smtp_host)
    .bind(587i64)
    .bind("xoauth2")
    .bind(&encrypted)
    .bind("lazy")
    .fetch_one(&user_db)
    .await?;

    state.sync_manager.start_account(
        account_id.clone(),
        user_id,
        std::sync::Arc::new(state.clone()),
    ).await;

    let redirect_base = std::env::var("APP_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".into());
    Ok(Redirect::temporary(&format!("{redirect_base}/mail/accounts?connected={account_id}")))
}
