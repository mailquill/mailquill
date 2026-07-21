use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::time::Duration;
use uuid::Uuid;

use crate::{error::AppError, middleware::UserId, state::AppState};

const MAX_ATTACHMENT_BYTES: usize = 25 * 1024 * 1024; // 25 MB
const BACKGROUND_SEND_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Deserialize)]
pub struct SendRequest {
    /// Local draft to remove after successful delivery.
    draft_id: Option<String>,
    /// account_id to send from
    account_id: String,
    /// from address (must be primary_email or an alias)
    from: String,
    to: Vec<String>,
    cc: Option<Vec<String>>,
    bcc: Option<Vec<String>>,
    subject: String,
    /// Plain text body
    body_text: Option<String>,
    /// HTML body
    body_html: Option<String>,
    /// PGP/MIME wrapper to build around the body.
    pgp_mime_mode: Option<String>,
    /// Detached armored signature for multipart/signed messages.
    pgp_signature: Option<String>,
    /// Optional iCalendar MIME method/body, used for meeting invitations and RSVP.
    calendar_method: Option<String>,
    calendar_ics: Option<String>,
    /// in_reply_to for replies (message_id_header of original)
    in_reply_to: Option<String>,
    /// references chain for replies/forwards
    references: Option<String>,
    /// Base64-encoded attachments
    attachments: Option<Vec<AttachmentInput>>,
}

#[derive(Deserialize)]
pub struct AttachmentInput {
    filename: String,
    content_type: String,
    /// base64-encoded data
    data: String,
}

#[derive(Serialize)]
pub struct SendResponse {
    send_id: String,
    status: &'static str,
}

#[derive(sqlx::FromRow)]
struct SendAccount {
    id: String,
    primary_email: String,
    credentials_encrypted: Vec<u8>,
    smtp_host: String,
    smtp_port: i64,
    smtp_auth_scheme: String,
    smtp_tls_cert: Option<String>,
    imap_host: Option<String>,
    imap_port: i64,
    imap_auth_scheme: String,
    imap_tls_cert: Option<String>,
    provider_kind: String,
}

pub async fn send_email(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<SendRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    // Keep only cheap validation in the request path. Provider connections,
    // token refresh and delivery happen after the client receives 202.
    let account: Option<SendAccount> = sqlx::query_as(
        "SELECT id, primary_email, credentials_encrypted, smtp_host, smtp_port, smtp_auth_scheme, smtp_tls_cert, imap_host, imap_port, imap_auth_scheme, imap_tls_cert, provider_kind FROM email_accounts WHERE id = ?",
    )
    .bind(&req.account_id)
    .fetch_optional(&user_db)
    .await?;

    let account = account.ok_or(AppError::NotFound)?;

    if let Some(draft_id) = req.draft_id.as_deref() {
        let valid_draft: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM messages WHERE id = ? AND account_id = ? AND is_local_draft = 1)",
        )
        .bind(draft_id)
        .bind(&req.account_id)
        .fetch_one(&user_db)
        .await?;
        if !valid_draft {
            return Err(AppError::NotFound);
        }
    }

    // Verify from is primary or an alias
    let is_primary = req.from.to_lowercase() == account.primary_email.to_lowercase();
    let is_alias: bool = if !is_primary {
        let exists: Option<String> = sqlx::query_scalar(
            "SELECT id FROM account_aliases WHERE account_id = ? AND LOWER(email) = LOWER(?)",
        )
        .bind(&account.id)
        .bind(&req.from)
        .fetch_optional(&user_db)
        .await?;
        exists.is_some()
    } else {
        false
    };

    if !is_primary && !is_alias {
        return Err(AppError::Forbidden);
    }

    // Validate attachment total size
    if let Some(ref attachments) = req.attachments {
        let total: usize = attachments
            .iter()
            .map(|a| a.data.len() * 3 / 4) // approximate decoded size
            .sum();
        if total > MAX_ATTACHMENT_BYTES {
            return Err(AppError::Unprocessable(
                "total attachment size exceeds 25 MB limit".into(),
            ));
        }
    }

    let send_id = Uuid::new_v4().to_string();
    let response_send_id = send_id.clone();
    let user_id = user.0;
    let subject = req.subject.clone();
    let draft_id = req.draft_id.clone();
    let event_state = state.clone();
    tokio::spawn(async move {
        let result = match tokio::time::timeout(
            BACKGROUND_SEND_TIMEOUT,
            deliver_email(&state, &user_db, req, account),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(AppError::BadGateway("send timed out".to_owned())),
        };
        match result {
            Ok(message_id) => {
                if let Some(draft_id) = draft_id {
                    if let Err(error) =
                        crate::routes::drafts::delete_local_draft(&event_state, &user_db, &draft_id)
                            .await
                    {
                        tracing::warn!(draft_id, error = %error, "sent draft cleanup failed");
                    }
                }
                publish_send_status(
                    &event_state,
                    &user_id,
                    &send_id,
                    "sent",
                    Some(&message_id),
                    Some(&subject),
                    None,
                )
            }
            Err(error) => {
                tracing::warn!(send_id, user_id, error = %error, "background send failed");
                let detail = send_failure_detail(&error);
                publish_send_status(
                    &event_state,
                    &user_id,
                    &send_id,
                    "failed",
                    None,
                    Some(&subject),
                    Some(&detail),
                );
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(SendResponse {
            send_id: response_send_id,
            status: "queued",
        }),
    ))
}

async fn deliver_email(
    state: &AppState,
    user_db: &SqlitePool,
    req: SendRequest,
    account: SendAccount,
) -> Result<String, AppError> {
    // Decrypt credentials
    let creds_bytes = state
        .credential_key
        .decrypt(&account.credentials_encrypted)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let creds: serde_json::Value =
        serde_json::from_slice(&creds_bytes).map_err(|e| AppError::Internal(e.to_string()))?;

    let smtp_user = creds["smtp_username"].as_str().unwrap_or("").to_owned();
    let smtp_pass = creds["smtp_password"].as_str().unwrap_or("").to_owned();
    // Prefer a freshly refreshed OAuth token over the (possibly expired) stored one.
    let oauth_token =
        match crate::oauth_tokens::fresh_access_token(&state.credential_key, user_db, &account.id)
            .await
        {
            Ok(Some(token)) => Some(token),
            Ok(None) => creds["oauth_access_token"].as_str().map(|s| s.to_owned()),
            Err(error) if crate::oauth_tokens::is_reauth_required(&error) => {
                return Err(AppError::Unprocessable(
                    "oauth_reauthentication_required".into(),
                ));
            }
            Err(error) => {
                return Err(AppError::BadGateway(format!(
                    "oauth token refresh failed: {error}"
                )));
            }
        };

    let smtp_request = smtp::SendRequest {
        from: req.from.clone(),
        to: req.to.clone(),
        cc: req.cc.clone().unwrap_or_default(),
        bcc: req.bcc.clone().unwrap_or_default(),
        subject: req.subject.clone(),
        body_text: req.body_text.clone(),
        body_html: req.body_html.clone(),
        pgp_mime_mode: req.pgp_mime_mode.clone(),
        pgp_signature: req.pgp_signature.clone(),
        calendar_method: req.calendar_method.clone(),
        calendar_ics: req.calendar_ics.clone(),
        in_reply_to: req.in_reply_to.clone(),
        references: req.references.clone(),
        attachments: req
            .attachments
            .as_ref()
            .map(|a| {
                a.iter()
                    .map(|att| smtp::AttachmentData {
                        filename: att.filename.clone(),
                        content_type: att.content_type.clone(),
                        data: att.data.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        smtp_host: account.smtp_host.clone(),
        smtp_port: account.smtp_port as u16,
        smtp_user: smtp_user.clone(),
        smtp_pass: smtp_pass.clone(),
        oauth_token: oauth_token.clone(),
        auth_scheme: account.smtp_auth_scheme.clone(),
        trusted_cert_der: mail_sync::session::decode_trusted_cert(account.smtp_tls_cert.as_deref()),
    };

    let kind = mail_sync::provider::ProviderKind::parse(&account.provider_kind);
    if !kind.sends_over_smtp() {
        // API accounts (Gmail / Graph): build the MIME message locally, deliver
        // through the provider's send endpoint. The provider stores the sent
        // copy server-side — no IMAP APPEND needed.
        let (message_id, raw_sent) = smtp::build_raw_with_id(&smtp_request)
            .map_err(|e| AppError::Internal(format!("message build failed: {e}")))?;

        let config = mail_sync::provider::ProviderConfig {
            oauth_access_token: oauth_token,
            db: Some(user_db.clone()),
            account_id: account.id.clone(),
            ..Default::default()
        };
        let mut provider = mail_sync::provider::connect(kind, &config)
            .await
            .map_err(|e| AppError::BadGateway(format!("provider connect failed: {e}")))?;
        provider
            .send_message(&raw_sent)
            .await
            .map_err(|e| AppError::BadGateway(format!("API send failed: {e}")))?;
        let _ = provider.close().await;

        return Ok(message_id);
    }

    // IMAP accounts: deliver via SMTP.
    let (message_id, raw_sent) = smtp::send(smtp_request)
        .await
        .map_err(|e| AppError::BadGateway(format!("SMTP send failed: {e}")))?;

    // Append sent message to IMAP Sent folder (task 7.4)
    if let Some(host) = account.imap_host.as_deref() {
        let imap_user = creds["imap_username"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&account.primary_email)
            .trim()
            .to_ascii_lowercase();
        let imap_pass = creds["imap_password"].as_str().unwrap_or("").to_owned();
        // APPEND is non-critical: SMTP delivery has already succeeded. Bound
        // this follow-up so a slow IMAP server cannot leave the UI in
        // "Sending…" indefinitely or encourage a duplicate retry.
        let trusted_imap_cert =
            mail_sync::session::decode_trusted_cert(account.imap_tls_cert.as_deref());
        let append = mail_sync::append_to_sent(
            host,
            account.imap_port as u16,
            &imap_user,
            &imap_pass,
            oauth_token.as_deref(),
            &account.imap_auth_scheme,
            trusted_imap_cert.as_deref(),
            &raw_sent,
        );
        match tokio::time::timeout(Duration::from_secs(15), append).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!("append to Sent failed after delivery: {error}"),
            Err(_) => tracing::warn!("append to Sent timed out after delivery"),
        }
    }

    Ok(message_id)
}

fn send_failure_detail(error: &AppError) -> String {
    match error {
        AppError::NotFound => "mailbox not found".to_owned(),
        AppError::Unauthorized => "authentication required".to_owned(),
        AppError::Forbidden => "sender address is not allowed".to_owned(),
        AppError::Conflict(message)
        | AppError::Unprocessable(message)
        | AppError::BadGateway(message) => message.clone(),
        AppError::BadGatewayWithCode { message, .. } => message.clone(),
        AppError::Internal(_) => "internal server error".to_owned(),
    }
}

#[allow(clippy::too_many_arguments)]
fn publish_send_status(
    state: &AppState,
    user_id: &str,
    send_id: &str,
    status: &str,
    message_id: Option<&str>,
    subject: Option<&str>,
    error: Option<&str>,
) {
    let _ = state.events.send(crate::state::UserEvent {
        user_id: user_id.to_owned(),
        event_type: "send".to_owned(),
        payload: serde_json::json!({
            "send_id": send_id,
            "status": status,
            "message_id": message_id,
            "subject": subject,
            "error": error,
        })
        .to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_failures_do_not_expose_internal_details() {
        let detail = send_failure_detail(&AppError::Internal(
            "database path and credential context".to_owned(),
        ));

        assert_eq!(detail, "internal server error");
    }

    #[test]
    fn provider_failures_keep_actionable_details() {
        let detail = send_failure_detail(&AppError::BadGateway(
            "SMTP authentication failed".to_owned(),
        ));

        assert_eq!(detail, "SMTP authentication failed");
    }
}
