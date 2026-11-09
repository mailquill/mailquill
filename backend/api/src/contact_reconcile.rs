//! Keeps the mailbox-managed contact source of an email account in step with
//! the account's credentials: picks the contact provider (Google People,
//! Microsoft Graph or CardDAV), derives its capability state and upserts the
//! `contact_accounts` row through `contact_sync`.

use contact_sync::{
    repository::{reconcile_mailbox_source, MailboxSource},
    ContactProvider, DavAuth,
};
use mailquill_core::crypto::CredentialKey;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::oauth_tokens::{contact_grant_state, ContactGrantState};

/// Result of reconciling a mailbox's contact source, as returned to clients.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ManagedContactCapability {
    /// Id of the `contact_accounts` row backing the mailbox source.
    pub source_id: String,
    /// The email account the source belongs to.
    pub email_account_id: String,
    /// Contact provider name (`google`, `graph`, `carddav`).
    pub provider: String,
    /// Capability state: `disabled`, `pending`, `consent_required`,
    /// `reauth_required` or `unavailable`.
    pub state: String,
    /// Machine-readable reason for a non-ready state, if any.
    pub reason: Option<String>,
    /// Whether syncing is enabled (never true for an `unavailable` source).
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderSelection {
    provider: ContactProvider,
    base_url: Option<String>,
}

/// Connection details for a mailbox-managed CardDAV source, built from the
/// email account's IMAP credentials and TLS settings.
pub struct ManagedCardDavContext {
    /// CardDAV base URL of the source.
    pub base_url: String,
    /// Basic auth derived from the mailbox username and password.
    pub auth: DavAuth,
    /// Pinned server certificate (DER) taken from the IMAP or SMTP exception.
    pub trusted_cert_der: Option<Vec<u8>>,
    /// Whether the account accepts invalid TLS certificates for DAV.
    pub accept_invalid_tls: bool,
}

/// Create or update the mailbox-managed contact source of `email_account_id`.
///
/// Decrypts the account credentials to select a provider (OAuth provider name
/// first, otherwise CardDAV when a password or explicit CardDAV URL exists),
/// defaults the CardDAV URL to the domain's `.well-known/carddav`, computes the
/// capability state and persists the source. `enabled_override` replaces the
/// stored enabled flag; without it the existing value (or `false`) is kept.
/// Errors are returned as plain strings for the caller to map.
pub async fn reconcile_mailbox_contact_source(
    db: &SqlitePool,
    credential_key: &CredentialKey,
    email_account_id: &str,
    enabled_override: Option<bool>,
) -> Result<ManagedContactCapability, String> {
    let row: Option<(String, String, String, Option<String>, Vec<u8>)> = sqlx::query_as(
        "SELECT display_name, primary_email, provider_kind, carddav_url, credentials_encrypted
         FROM email_accounts WHERE id = ?",
    )
    .bind(email_account_id)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?;
    let (display_name, primary_email, provider_kind, carddav_url, encrypted) =
        row.ok_or_else(|| "mailbox not found".to_owned())?;
    let credentials = decrypt_credentials(credential_key, &encrypted)?;
    let selection = select_provider(&provider_kind, carddav_url.as_deref(), &credentials);
    let existing_enabled: Option<bool> =
        sqlx::query_scalar("SELECT enabled FROM contact_accounts WHERE email_account_id = ?")
            .bind(email_account_id)
            .fetch_optional(db)
            .await
            .map_err(|error| error.to_string())?;
    let enabled = enabled_override.or(existing_enabled).unwrap_or(false);

    let (provider, base_url, state, reason) = match selection {
        Some(mut selection) => {
            if selection.provider == ContactProvider::CardDav && selection.base_url.is_none() {
                selection.base_url = primary_email
                    .split_once('@')
                    .map(|(_, domain)| format!("https://{domain}/.well-known/carddav"));
            }
            let (state, reason) = capability_state(&selection.provider, &credentials, enabled);
            (selection.provider, selection.base_url, state, reason)
        }
        None => (
            ContactProvider::CardDav,
            None,
            "unavailable".to_owned(),
            Some("no_supported_contact_provider".to_owned()),
        ),
    };
    let source = MailboxSource {
        email_account_id: email_account_id.to_owned(),
        display_name: format!("{display_name} contacts"),
        provider: provider.clone(),
        base_url,
        enabled: enabled && state != "unavailable",
        capability_state: state.clone(),
        capability_reason: reason.clone(),
    };
    let source_id = reconcile_mailbox_source(db, &source)
        .await
        .map_err(|error| error.to_string())?;

    Ok(ManagedContactCapability {
        source_id,
        email_account_id: email_account_id.to_owned(),
        provider: provider.as_str().to_owned(),
        state,
        reason,
        enabled: source.enabled,
    })
}

fn capability_state(
    provider: &ContactProvider,
    credentials: &serde_json::Value,
    enabled: bool,
) -> (String, Option<String>) {
    if !enabled {
        return ("disabled".to_owned(), None);
    }
    match provider {
        ContactProvider::Google | ContactProvider::Graph => {
            match contact_grant_state(credentials) {
                ContactGrantState::Ready => ("pending".to_owned(), None),
                ContactGrantState::ConsentRequired => (
                    "consent_required".to_owned(),
                    Some("contact_scope_missing".to_owned()),
                ),
                ContactGrantState::ReauthenticationRequired => (
                    "reauth_required".to_owned(),
                    Some("oauth_reauthentication_required".to_owned()),
                ),
            }
        }
        ContactProvider::CardDav => ("pending".to_owned(), None),
    }
}

fn select_provider(
    _provider_kind: &str,
    carddav_url: Option<&str>,
    credentials: &serde_json::Value,
) -> Option<ProviderSelection> {
    match credentials["provider"].as_str() {
        Some("google") => Some(ProviderSelection {
            provider: ContactProvider::Google,
            base_url: None,
        }),
        Some("microsoft") => Some(ProviderSelection {
            provider: ContactProvider::Graph,
            base_url: None,
        }),
        _ if has_password(credentials) || carddav_url.is_some() => {
            let base_url = carddav_url.map(str::to_owned);
            Some(ProviderSelection {
                provider: ContactProvider::CardDav,
                base_url,
            })
        }
        _ => None,
    }
}

fn has_password(credentials: &serde_json::Value) -> bool {
    ["imap_password", "password"].iter().any(|field| {
        credentials[*field]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    })
}

fn decrypt_credentials(
    credential_key: &CredentialKey,
    encrypted: &[u8],
) -> Result<serde_json::Value, String> {
    let decrypted = credential_key
        .decrypt(encrypted)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&decrypted).map_err(|error| error.to_string())
}

/// Load the CardDAV connection context of a mailbox-managed source.
///
/// Only sources with `management_mode = 'mailbox'` qualify; the username and
/// password come from the mailbox credentials (falling back to the primary
/// email as username). Fails when the source is missing or the credentials
/// hold no password.
pub async fn managed_carddav_context(
    db: &SqlitePool,
    credential_key: &CredentialKey,
    source_id: &str,
) -> Result<ManagedCardDavContext, String> {
    let row: Option<(
        String,
        String,
        Vec<u8>,
        Option<String>,
        Option<String>,
        bool,
    )> = sqlx::query_as(
        "SELECT source.base_url, mailbox.primary_email, mailbox.credentials_encrypted,
                    mailbox.imap_tls_cert, mailbox.smtp_tls_cert,
                    mailbox.caldav_accept_invalid_tls
             FROM contact_accounts AS source
             JOIN email_accounts AS mailbox ON mailbox.id = source.email_account_id
             WHERE source.id = ? AND source.management_mode = 'mailbox' AND source.type = 'cardav'",
    )
    .bind(source_id)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?;
    let (base_url, primary_email, encrypted, imap_cert, smtp_cert, accept_invalid_tls) =
        row.ok_or_else(|| "managed CardDAV source not found".to_owned())?;
    let credentials = decrypt_credentials(credential_key, &encrypted)?;
    let username = credentials["imap_username"]
        .as_str()
        .or_else(|| credentials["username"].as_str())
        .unwrap_or(&primary_email)
        .to_owned();
    let password = credentials["imap_password"]
        .as_str()
        .or_else(|| credentials["password"].as_str())
        .ok_or_else(|| "mailbox credentials do not contain a CardDAV password".to_owned())?
        .to_owned();
    Ok(ManagedCardDavContext {
        base_url,
        auth: DavAuth::Basic { username, password },
        trusted_cert_der: mail_sync::session::decode_trusted_cert(
            imap_cert.as_deref().or(smtp_cert.as_deref()),
        ),
        accept_invalid_tls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth_tokens::{GOOGLE_CONTACTS_SCOPE, MICROSOFT_CONTACTS_SCOPE};
    use sqlx::sqlite::SqlitePoolOptions;

    fn key() -> CredentialKey {
        CredentialKey([7; 32])
    }

    async fn database(credentials: serde_json::Value, provider_kind: &str) -> SqlitePool {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db::migrations::run_mail_migrations(&db).await.unwrap();
        let encrypted = key()
            .encrypt(&serde_json::to_vec(&credentials).unwrap())
            .unwrap();
        sqlx::query(
            "INSERT INTO email_accounts
             (id, display_name, primary_email, imap_host, imap_port, imap_auth_scheme,
              smtp_host, smtp_port, smtp_auth_scheme, credentials_encrypted, provider_kind)
             VALUES ('mailbox', 'Mailbox', 'owner@example.com', 'imap.example.com', 993,
                     'plain', 'smtp.example.com', 465, 'plain', ?, ?)",
        )
        .bind(encrypted)
        .bind(provider_kind)
        .execute(&db)
        .await
        .unwrap();
        db
    }

    #[tokio::test]
    async fn selects_oauth_providers_and_classifies_scope_state() {
        for (provider, scope, expected) in [
            ("google", GOOGLE_CONTACTS_SCOPE, "pending"),
            ("microsoft", MICROSOFT_CONTACTS_SCOPE, "pending"),
        ] {
            let db = database(
                serde_json::json!({
                    "provider": provider,
                    "oauth_granted_scopes": [scope]
                }),
                "imap",
            )
            .await;
            let result = reconcile_mailbox_contact_source(&db, &key(), "mailbox", Some(true))
                .await
                .unwrap();
            assert_eq!(result.state, expected);
            assert_eq!(
                result.provider,
                if provider == "google" {
                    "google"
                } else {
                    "graph"
                }
            );
            let copied_credentials: i64 = sqlx::query_scalar(
                "SELECT length(credentials_encrypted) FROM contact_accounts WHERE id = ?",
            )
            .bind(&result.source_id)
            .fetch_one(&db)
            .await
            .unwrap();
            assert_eq!(copied_credentials, 0);
        }
    }

    #[tokio::test]
    async fn missing_scope_and_reauthentication_are_actionable() {
        let db = database(
            serde_json::json!({
                "provider": "google",
                "oauth_granted_scopes": ["https://mail.google.com/"]
            }),
            "gmail_api",
        )
        .await;
        let missing = reconcile_mailbox_contact_source(&db, &key(), "mailbox", Some(true))
            .await
            .unwrap();
        assert_eq!(missing.state, "consent_required");
        assert_eq!(missing.reason.as_deref(), Some("contact_scope_missing"));

        let encrypted = key()
            .encrypt(
                &serde_json::to_vec(&serde_json::json!({
                    "provider": "google",
                    "oauth_granted_scopes": [GOOGLE_CONTACTS_SCOPE],
                    "oauth_reauth_required": true
                }))
                .unwrap(),
            )
            .unwrap();
        sqlx::query("UPDATE email_accounts SET credentials_encrypted = ? WHERE id = 'mailbox'")
            .bind(encrypted)
            .execute(&db)
            .await
            .unwrap();
        let revoked = reconcile_mailbox_contact_source(&db, &key(), "mailbox", Some(true))
            .await
            .unwrap();
        assert_eq!(revoked.source_id, missing.source_id);
        assert_eq!(revoked.state, "reauth_required");
    }

    #[tokio::test]
    async fn generic_mailbox_selects_carddav_without_copying_credentials() {
        let db = database(
            serde_json::json!({
                "imap_username": "owner@example.com",
                "imap_password": "secret"
            }),
            "imap",
        )
        .await;
        sqlx::query("UPDATE email_accounts SET carddav_url = ? WHERE id = 'mailbox'")
            .bind("https://dav.example.com/addressbooks/owner/")
            .execute(&db)
            .await
            .unwrap();
        let result = reconcile_mailbox_contact_source(&db, &key(), "mailbox", Some(true))
            .await
            .unwrap();
        assert_eq!(result.provider, "cardav");
        let context = managed_carddav_context(&db, &key(), &result.source_id)
            .await
            .unwrap();
        assert_eq!(
            context.base_url,
            "https://dav.example.com/addressbooks/owner/"
        );
        assert!(matches!(context.auth, DavAuth::Basic { .. }));
    }

    #[tokio::test]
    async fn unsupported_mailbox_is_unavailable_and_reconciliation_is_idempotent() {
        let db = database(serde_json::json!({}), "unsupported").await;
        let first = reconcile_mailbox_contact_source(&db, &key(), "mailbox", Some(true))
            .await
            .unwrap();
        let second = reconcile_mailbox_contact_source(&db, &key(), "mailbox", None)
            .await
            .unwrap();
        assert_eq!(first.source_id, second.source_id);
        assert_eq!(first.state, "unavailable");
        assert_eq!(second.state, "unavailable");
    }
}
