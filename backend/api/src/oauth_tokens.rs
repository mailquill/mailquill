//! Central OAuth access-token refresh, shared by sync, on-demand fetches, and
//! send. Tokens live in the account's encrypted credentials JSON:
//! `oauth_access_token`, `oauth_refresh_token`, `provider` (google/microsoft),
//! `oauth_expires_at` (epoch seconds). A token is refreshed when it expires
//! within the next 5 minutes; the rotated refresh token (if any) is persisted.

use mailquill_core::crypto::CredentialKey;
use sqlx::SqlitePool;
use tracing::{info, warn};

const EXPIRY_SLACK_SECS: i64 = 300;

/// Return a currently valid OAuth access token for the account, refreshing it
/// first if it is (about to be) expired. `Ok(None)` for accounts without
/// OAuth credentials. Refresh failures fall back to the stored token — the
/// provider call will then fail with an auth error instead of masking it here.
pub async fn fresh_access_token(
    credential_key: &CredentialKey,
    user_db: &SqlitePool,
    account_id: &str,
) -> Result<Option<String>, String> {
    let creds_enc: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT credentials_encrypted FROM email_accounts WHERE id = ?")
            .bind(account_id)
            .fetch_optional(user_db)
            .await
            .map_err(|e| e.to_string())?;
    let creds_enc = creds_enc.ok_or("account not found")?;

    let creds_bytes = credential_key.decrypt(&creds_enc).map_err(|e| e.to_string())?;
    let mut creds: serde_json::Value =
        serde_json::from_slice(&creds_bytes).map_err(|e| e.to_string())?;

    let access_token = creds["oauth_access_token"].as_str().unwrap_or("").to_owned();
    let refresh_token = creds["oauth_refresh_token"].as_str().unwrap_or("").to_owned();
    if access_token.is_empty() && refresh_token.is_empty() {
        return Ok(None);
    }
    if refresh_token.is_empty() {
        return Ok(Some(access_token));
    }

    let expires_at = creds["oauth_expires_at"].as_i64().unwrap_or(0);
    let now = chrono::Utc::now().timestamp();
    if expires_at > now + EXPIRY_SLACK_SECS {
        return Ok(Some(access_token));
    }

    let provider = creds["provider"].as_str().unwrap_or("").to_owned();
    match refresh(&provider, &refresh_token).await {
        Ok(refreshed) => {
            creds["oauth_access_token"] = serde_json::json!(refreshed.access_token);
            creds["oauth_expires_at"] = serde_json::json!(now + refreshed.expires_in);
            if let Some(rotated) = refreshed.refresh_token {
                creds["oauth_refresh_token"] = serde_json::json!(rotated);
            }

            let encrypted = credential_key
                .encrypt(&serde_json::to_vec(&creds).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            sqlx::query("UPDATE email_accounts SET credentials_encrypted = ? WHERE id = ?")
                .bind(&encrypted)
                .bind(account_id)
                .execute(user_db)
                .await
                .map_err(|e| e.to_string())?;

            info!("oauth: refreshed {provider} token for account {account_id}");
            Ok(Some(refreshed.access_token))
        }
        Err(e) => {
            warn!("oauth: token refresh failed for account {account_id}: {e}");
            Ok(Some(access_token))
        }
    }
}

struct Refreshed {
    access_token: String,
    expires_in: i64,
    refresh_token: Option<String>,
}

async fn refresh(provider: &str, refresh_token: &str) -> Result<Refreshed, String> {
    let (token_url, env_prefix) = match provider {
        "google" => ("https://oauth2.googleapis.com/token", "GOOGLE"),
        "microsoft" => (
            "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            "MICROSOFT",
        ),
        other => return Err(format!("unknown oauth provider: {other}")),
    };
    // Same env convention as the OAuth login flow (routes/oauth.rs).
    let client_id = std::env::var(format!("{env_prefix}_OAUTH_CLIENT_ID"))
        .map_err(|_| format!("{env_prefix}_OAUTH_CLIENT_ID not configured"))?;
    let client_secret = std::env::var(format!("{env_prefix}_OAUTH_CLIENT_SECRET"))
        .map_err(|_| format!("{env_prefix}_OAUTH_CLIENT_SECRET not configured"))?;

    let res = reqwest::Client::new()
        .post(token_url)
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = res.status();
    let body: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("token endpoint returned {status}: {body}"));
    }

    Ok(Refreshed {
        access_token: body["access_token"]
            .as_str()
            .ok_or("no access_token in refresh response")?
            .to_owned(),
        expires_in: body["expires_in"].as_i64().unwrap_or(3600),
        refresh_token: body["refresh_token"].as_str().map(str::to_owned),
    })
}
