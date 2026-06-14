use axum::{
    extract::{Extension, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{error::AppError, middleware::UserId, state::AppState};

const MAX_ATTACHMENT_BYTES: usize = 25 * 1024 * 1024; // 25 MB

#[derive(Deserialize)]
pub struct SendRequest {
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
    message_id: String,
}

pub async fn send_email(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<SendRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    // Validate from address ownership (task 7.1)
    let account: Option<(String, String, Vec<u8>, String, i64, String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT id, primary_email, credentials_encrypted, smtp_host, smtp_port, smtp_auth_scheme, smtp_tls_cert, imap_tls_cert, provider_kind FROM email_accounts WHERE id = ?",
    )
    .bind(&req.account_id)
    .fetch_optional(&user_db)
    .await?;

    let (account_id, primary_email, creds_enc, smtp_host, smtp_port, smtp_auth_scheme, smtp_tls_cert, imap_tls_cert, provider_kind) =
        account.ok_or(AppError::NotFound)?;

    // Verify from is primary or an alias
    let is_primary = req.from.to_lowercase() == primary_email.to_lowercase();
    let is_alias: bool = if !is_primary {
        let exists: Option<String> = sqlx::query_scalar(
            "SELECT id FROM account_aliases WHERE account_id = ? AND LOWER(email) = LOWER(?)",
        )
        .bind(&account_id)
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

    // Decrypt credentials
    let creds_bytes = state
        .credential_key
        .decrypt(&creds_enc)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let creds: serde_json::Value =
        serde_json::from_slice(&creds_bytes).map_err(|e| AppError::Internal(e.to_string()))?;

    let smtp_user = creds["smtp_username"].as_str().unwrap_or("").to_owned();
    let smtp_pass = creds["smtp_password"].as_str().unwrap_or("").to_owned();
    // Prefer a freshly refreshed OAuth token over the (possibly expired) stored one.
    let oauth_token =
        match crate::oauth_tokens::fresh_access_token(&state.credential_key, &user_db, &account_id)
            .await
        {
            Ok(Some(token)) => Some(token),
            _ => creds["oauth_access_token"].as_str().map(|s| s.to_owned()),
        };

    let smtp_request = smtp::SendRequest {
        from: req.from.clone(),
        to: req.to.clone(),
        cc: req.cc.clone().unwrap_or_default(),
        bcc: req.bcc.clone().unwrap_or_default(),
        subject: req.subject.clone(),
        body_text: req.body_text.clone(),
        body_html: req.body_html.clone(),
        in_reply_to: req.in_reply_to.clone(),
        references: req.references.clone(),
        attachments: req.attachments.as_ref().map(|a| {
            a.iter()
                .map(|att| smtp::AttachmentData {
                    filename: att.filename.clone(),
                    content_type: att.content_type.clone(),
                    data: att.data.clone(),
                })
                .collect()
        }).unwrap_or_default(),
        smtp_host: smtp_host.clone(),
        smtp_port: smtp_port as u16,
        smtp_user: smtp_user.clone(),
        smtp_pass: smtp_pass.clone(),
        oauth_token: oauth_token.clone(),
        auth_scheme: smtp_auth_scheme.clone(),
        trusted_cert_der: mail_sync::session::decode_trusted_cert(smtp_tls_cert.as_deref()),
    };

    let kind = mail_sync::provider::ProviderKind::parse(&provider_kind);
    if !kind.sends_over_smtp() {
        // API accounts (Gmail / Graph): build the MIME message locally, deliver
        // through the provider's send endpoint. The provider stores the sent
        // copy server-side — no IMAP APPEND needed.
        let (message_id, raw_sent) = smtp::build_raw_with_id(&smtp_request)
            .map_err(|e| AppError::Internal(format!("message build failed: {e}")))?;

        let config = mail_sync::provider::ProviderConfig {
            oauth_access_token: oauth_token,
            db: Some(user_db.clone()),
            account_id: account_id.clone(),
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

        return Ok(Json(SendResponse { message_id }));
    }

    // IMAP accounts: deliver via SMTP.
    let (message_id, raw_sent) = smtp::send(smtp_request)
        .await
        .map_err(|e| AppError::BadGateway(format!("SMTP send failed: {e}")))?;

    // Append sent message to IMAP Sent folder (task 7.4)
    let imap_host: Option<String> = sqlx::query_scalar(
        "SELECT imap_host FROM email_accounts WHERE id = ?",
    )
    .bind(&account_id)
    .fetch_optional(&user_db)
    .await?;

    if let Some(host) = imap_host {
        let imap_port: i64 = sqlx::query_scalar(
            "SELECT imap_port FROM email_accounts WHERE id = ?",
        )
        .bind(&account_id)
        .fetch_one(&user_db)
        .await
        .unwrap_or(993);

        let imap_user = creds["imap_username"].as_str().unwrap_or("").to_owned();
        let imap_pass = creds["imap_password"].as_str().unwrap_or("").to_owned();
        let imap_auth = sqlx::query_scalar::<_, String>(
            "SELECT imap_auth_scheme FROM email_accounts WHERE id = ?",
        )
        .bind(&account_id)
        .fetch_optional(&user_db)
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| "plain".to_string());

        // Fire-and-forget APPEND (non-critical)
        let _ = mail_sync::append_to_sent(
            &host,
            imap_port as u16,
            &imap_user,
            &imap_pass,
            oauth_token.as_deref(),
            &imap_auth,
            mail_sync::session::decode_trusted_cert(imap_tls_cert.as_deref()).as_deref(),
            &raw_sent,
        )
        .await;
    }

    Ok(Json(SendResponse { message_id }))
}
