use axum::{
    extract::{Path, Query, State},
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

#[derive(Deserialize)]
pub struct OAuthStartQuery {
    /// Short-lived access token, passed in the query because a full-page
    /// redirect to the provider cannot send an Authorization header.
    token: String,
    /// Existing account to reconnect after a revoked refresh token.
    account_id: Option<String>,
    /// Return to the calendar after consent and activate Google Calendar sync.
    calendar: Option<bool>,
    /// Request and enable the mailbox contact capability, preserving return context.
    contacts: Option<bool>,
}

#[derive(Debug)]
struct OAuthProfile {
    email: String,
    name: Option<String>,
}

/// Temporary PKCE verifier store (in-memory, keyed by CSRF state token).
/// Production use: persist in Redis or app DB.
pub type PkceStore = Mutex<
    HashMap<
        String,
        (
            String,
            PkceCodeVerifier,
            String,
            Option<String>,
            bool,
            Option<bool>,
        ),
    >,
>;

pub fn new_pkce_store() -> PkceStore {
    Mutex::new(HashMap::new())
}

fn provider_config(provider: &str) -> Result<(oauth2::AuthUrl, oauth2::TokenUrl), AppError> {
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
            Scope::new("https://www.googleapis.com/auth/calendar.events".into()),
            Scope::new("https://www.googleapis.com/auth/calendar.calendarlist.readonly".into()),
            Scope::new("https://www.googleapis.com/auth/contacts".into()),
            Scope::new("email".into()),
            Scope::new("profile".into()),
        ],
        // Graph API scopes (accounts run as provider_kind = outlook_api). The
        // v2 endpoint issues a token for ONE resource; switching this account
        // to IMAP/SMTP would need re-consent with outlook.office.com scopes.
        PROVIDER_MICROSOFT => vec![
            Scope::new("https://graph.microsoft.com/Mail.ReadWrite".into()),
            Scope::new("https://graph.microsoft.com/Mail.Send".into()),
            Scope::new("https://graph.microsoft.com/Contacts.ReadWrite".into()),
            Scope::new("https://graph.microsoft.com/User.Read".into()),
            Scope::new("offline_access".into()),
            Scope::new("email".into()),
        ],
        _ => vec![],
    }
}

fn mail_provider_kind(provider: &str) -> Result<&'static str, AppError> {
    match provider {
        PROVIDER_GOOGLE => Ok("gmail_imap"),
        PROVIDER_MICROSOFT => Ok("imap"),
        _ => Err(AppError::NotFound),
    }
}

fn build_client(provider: &str, state: &AppState) -> Result<oauth2::basic::BasicClient, AppError> {
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
    let redirect_base =
        std::env::var("APP_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".into());
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
    Path(provider): Path<String>,
    Query(query): Query<OAuthStartQuery>,
    axum::extract::Extension(pkce_store): axum::extract::Extension<std::sync::Arc<PkceStore>>,
) -> Result<impl IntoResponse, AppError> {
    let user = UserId(
        state
            .jwt_key
            .validate(&query.token)
            .map_err(|_| AppError::Unauthorized)?,
    );
    if let Some(account_id) = query.account_id.as_deref() {
        let user_db = state.user_db_pool.get(&user.0).await?;
        let credentials: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT credentials_encrypted FROM email_accounts WHERE id = ?")
                .bind(account_id)
                .fetch_optional(&user_db)
                .await?;
        let credentials = credentials.ok_or(AppError::NotFound)?;
        let decrypted = state
            .credential_key
            .decrypt(&credentials)
            .map_err(|error| AppError::Internal(error.to_string()))?;
        let stored: serde_json::Value = serde_json::from_slice(&decrypted)
            .map_err(|error| AppError::Internal(error.to_string()))?;
        if stored["provider"].as_str() != Some(provider.as_str()) {
            return Err(AppError::Unprocessable(
                "OAuth provider does not match the account".into(),
            ));
        }
    }
    let client = build_client(&provider, &state)?;
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let scopes = provider_scopes(&provider);
    let mut builder = client.authorize_url(CsrfToken::new_random);
    for scope in scopes {
        builder = builder.add_scope(scope);
    }
    if provider == PROVIDER_GOOGLE {
        // Google only issues a refresh token with offline access + forced
        // consent; without it the token expires after an hour for good.
        builder = builder
            .add_extra_param("access_type", "offline")
            .add_extra_param("prompt", "consent");
    }
    let (auth_url, csrf_token) = builder.set_pkce_challenge(pkce_challenge).url();

    let state_val = csrf_token.secret().clone();
    pkce_store.lock().await.insert(
        state_val.clone(),
        (
            provider.clone(),
            pkce_verifier,
            user.0.clone(),
            query.account_id,
            query.calendar.unwrap_or(false),
            query.contacts,
        ),
    );

    Ok(Redirect::temporary(auth_url.as_str()))
}

pub async fn oauth_callback(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(params): Query<OAuthCallbackQuery>,
    axum::extract::Extension(pkce_store): axum::extract::Extension<std::sync::Arc<PkceStore>>,
) -> Result<impl IntoResponse, AppError> {
    let (
        stored_provider,
        pkce_verifier,
        user_id,
        reconnect_account_id,
        calendar_requested,
        contacts_requested,
    ) = pkce_store
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
    let expires_at = chrono::Utc::now().timestamp()
        + token_result
            .expires_in()
            .map(|d| d.as_secs() as i64)
            .unwrap_or(3600);
    let granted_scopes = token_result
        .scopes()
        .map(|scopes| {
            scopes
                .iter()
                .map(|scope| scope.as_str().to_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            provider_scopes(&provider)
                .into_iter()
                .map(|scope| scope.as_str().to_owned())
                .collect()
        });

    let profile = fetch_oauth_profile(&provider, &access_token).await?;
    crate::validate::email("oauth email", &profile.email)?;
    let display_email = profile.email.to_lowercase();

    // Encrypt and store OAuth refresh token
    let creds = serde_json::json!({
        "imap_username": display_email,
        "smtp_username": display_email,
        "oauth_access_token": access_token,
        "oauth_refresh_token": refresh_token,
        "oauth_expires_at": expires_at,
        "oauth_granted_scopes": granted_scopes,
        "oauth_reauth_required": false,
        "provider": provider,
    });
    let creds_bytes = serde_json::to_vec(&creds).unwrap();
    let encrypted = state
        .credential_key
        .encrypt(&creds_bytes)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let user_db = state.user_db_pool.get(&user_id).await?;

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

    // Gmail mail transport stays on IMAP/XOAUTH2 so the account can use IMAP
    // IDLE for immediate delivery. The hybrid provider uses the Gmail API only
    // for label semantics, correlated through X-GM-MSGID.
    let provider_kind = mail_provider_kind(&provider)?;

    let display_name = oauth_display_name(&provider, &profile, &display_email);
    let pending_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM email_accounts WHERE primary_email = 'oauth@pending' AND provider_kind = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(provider_kind)
    .fetch_optional(&user_db)
    .await?;

    let account_id = if let Some(reconnect_id) = reconnect_account_id {
        let current_email: Option<String> =
            sqlx::query_scalar("SELECT primary_email FROM email_accounts WHERE id = ?")
                .bind(&reconnect_id)
                .fetch_optional(&user_db)
                .await?;
        let current_email = current_email.ok_or(AppError::NotFound)?;
        if !current_email.eq_ignore_ascii_case(&display_email) {
            return Err(AppError::Unprocessable(
                "OAuth identity does not match the account email".into(),
            ));
        }
        sqlx::query(
            "UPDATE email_accounts SET imap_host = ?, imap_port = ?, imap_auth_scheme = ?, smtp_host = ?, smtp_port = ?, smtp_auth_scheme = ?, credentials_encrypted = ?, provider_kind = ?, sync_mode = 'idle' WHERE id = ?",
        )
        .bind(imap_host)
        .bind(993i64)
        .bind("xoauth2")
        .bind(smtp_host)
        .bind(587i64)
        .bind("xoauth2")
        .bind(&encrypted)
        .bind(provider_kind)
        .bind(&reconnect_id)
        .execute(&user_db)
        .await?;
        reconnect_id
    } else if let Some(pending_id) = pending_id {
        sqlx::query(
            "UPDATE email_accounts SET display_name = ?, primary_email = ?, imap_host = ?, imap_port = ?, imap_auth_scheme = ?, smtp_host = ?, smtp_port = ?, smtp_auth_scheme = ?, credentials_encrypted = ?, body_sync_mode = ?, provider_kind = ?, sync_mode = 'idle' WHERE id = ?",
        )
        .bind(&display_name)
        .bind(&display_email)
        .bind(imap_host)
        .bind(993i64)
        .bind("xoauth2")
        .bind(smtp_host)
        .bind(587i64)
        .bind("xoauth2")
        .bind(&encrypted)
        .bind("lazy")
        .bind(provider_kind)
        .bind(&pending_id)
        .execute(&user_db)
        .await?;
        pending_id
    } else {
        sqlx::query_scalar(
            "INSERT INTO email_accounts (display_name, primary_email, imap_host, imap_port, imap_auth_scheme, smtp_host, smtp_port, smtp_auth_scheme, credentials_encrypted, body_sync_mode, provider_kind, sync_mode) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'idle') RETURNING id",
        )
        .bind(&display_name)
        .bind(&display_email)
        .bind(imap_host)
        .bind(993i64)
        .bind("xoauth2")
        .bind(smtp_host)
        .bind(587i64)
        .bind("xoauth2")
        .bind(&encrypted)
        .bind("lazy")
        .bind(provider_kind)
        .fetch_one(&user_db)
        .await?
    };

    if provider == PROVIDER_GOOGLE {
        if let Err(error) = crate::routes::calendar::connect_google_account(
            &user_db,
            &state,
            &account_id,
            &display_name,
            &encrypted,
        )
        .await
        {
            tracing::warn!(account_id, %error, "google calendar connection failed");
        }
    }

    if let Ok(capability) = crate::contact_reconcile::reconcile_mailbox_contact_source(
        &user_db,
        &state.credential_key,
        &account_id,
        Some(contacts_requested.unwrap_or(true)),
    )
    .await
    {
        if capability.enabled && capability.state == "pending" {
            crate::routes::contacts::spawn_contact_sync_task(
                state.clone(),
                user_id.clone(),
                capability.source_id,
                true,
            )
            .await;
        }
    }

    state
        .sync_manager
        .start_account(
            account_id.clone(),
            user_id,
            std::sync::Arc::new(state.clone()),
        )
        .await;

    let redirect_base =
        std::env::var("APP_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".into());
    let destination = if contacts_requested.is_some() {
        format!("/mail/accounts?connected={account_id}&contacts=complete")
    } else if calendar_requested && provider == PROVIDER_GOOGLE {
        format!("/mail/calendar?connected={account_id}")
    } else {
        format!("/mail/accounts?connected={account_id}")
    };
    Ok(Redirect::temporary(&format!(
        "{redirect_base}{destination}"
    )))
}

async fn fetch_oauth_profile(provider: &str, access_token: &str) -> Result<OAuthProfile, AppError> {
    let client = reqwest::Client::new();
    let url = match provider {
        PROVIDER_GOOGLE => "https://openidconnect.googleapis.com/v1/userinfo",
        PROVIDER_MICROSOFT => "https://graph.microsoft.com/v1.0/me",
        _ => return Err(AppError::NotFound),
    };

    let body: serde_json::Value = client
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| AppError::BadGateway(format!("oauth profile request: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::BadGateway(format!("oauth profile response: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::BadGateway(format!("oauth profile parse: {e}")))?;

    let email = match provider {
        PROVIDER_GOOGLE => body["email"].as_str(),
        PROVIDER_MICROSOFT => body["mail"]
            .as_str()
            .or_else(|| body["userPrincipalName"].as_str()),
        _ => None,
    }
    .filter(|s| !s.trim().is_empty())
    .ok_or_else(|| AppError::BadGateway("oauth profile did not include an email".into()))?
    .trim()
    .to_owned();

    let name = match provider {
        PROVIDER_GOOGLE => body["name"].as_str(),
        PROVIDER_MICROSOFT => body["displayName"].as_str(),
        _ => None,
    }
    .filter(|s| !s.trim().is_empty())
    .map(|s| s.trim().to_owned());

    Ok(OAuthProfile { email, name })
}

fn oauth_display_name(provider: &str, profile: &OAuthProfile, email: &str) -> String {
    let provider_name = if provider == PROVIDER_GOOGLE {
        "Gmail"
    } else {
        "Outlook"
    };
    match profile.name.as_deref() {
        Some(name) => format!("{name} ({email})"),
        None => format!("{provider_name} ({email})"),
    }
}

#[cfg(test)]
mod tests {
    use super::{mail_provider_kind, PROVIDER_GOOGLE, PROVIDER_MICROSOFT};

    #[test]
    fn google_oauth_uses_gmail_imap_hybrid() {
        assert_eq!(mail_provider_kind(PROVIDER_GOOGLE).unwrap(), "gmail_imap");
        assert_eq!(mail_provider_kind(PROVIDER_MICROSOFT).unwrap(), "imap");
    }
}
