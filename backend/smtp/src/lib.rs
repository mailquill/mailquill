use base64::Engine;
use lettre::{
    message::{header::ContentType, Mailbox, MultiPart, SinglePart},
    transport::smtp::{
        authentication::{Credentials, Mechanism},
        client::{Tls, TlsParameters},
    },
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

#[derive(Debug, thiserror::Error)]
pub enum SmtpError {
    #[error("build error: {0}")]
    Build(String),
    #[error("send error: {0}")]
    Send(String),
    #[error("attachment decode error: {0}")]
    Attachment(String),
}

pub struct AttachmentData {
    pub filename: String,
    pub content_type: String,
    pub data: String, // base64-encoded
}

pub struct SendRequest {
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    pub attachments: Vec<AttachmentData>,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_user: String,
    pub smtp_pass: String,
    pub oauth_token: Option<String>,
    pub auth_scheme: String,
    /// User-approved TLS trust exception (DER certificate). Added as a trust
    /// anchor with hostname verification disabled; `None` keeps strict WebPKI
    /// verification.
    pub trusted_cert_der: Option<Vec<u8>>,
}

/// Send an email and return (Message-ID, raw bytes for IMAP APPEND).
pub async fn send(req: SendRequest) -> Result<(String, Vec<u8>), SmtpError> {
    let message = build_message(&req)?;
    let raw = message.formatted();

    let message_id = message
        .headers()
        .get_raw("Message-ID")
        .unwrap_or("")
        .trim()
        .to_owned();

    let transport = build_transport(&req)?;
    transport
        .send(message)
        .await
        .map_err(|e| SmtpError::Send(e.to_string()))?;

    Ok((message_id, raw))
}

/// Build raw RFC 2822 bytes without sending — used for IMAP APPEND.
pub fn build_raw_message(req: &SendRequest) -> Result<Vec<u8>, SmtpError> {
    Ok(build_message(req)?.formatted())
}

/// Build the message without sending and return (Message-ID, raw bytes) —
/// used when delivery happens through a provider API instead of SMTP.
pub fn build_raw_with_id(req: &SendRequest) -> Result<(String, Vec<u8>), SmtpError> {
    let message = build_message(req)?;
    let raw = message.formatted();
    let message_id = message
        .headers()
        .get_raw("Message-ID")
        .unwrap_or("")
        .trim()
        .to_owned();
    Ok((message_id, raw))
}

fn build_message(req: &SendRequest) -> Result<Message, SmtpError> {
    let from: Mailbox = req
        .from
        .parse()
        .map_err(|e: lettre::address::AddressError| SmtpError::Build(e.to_string()))?;
    let mut builder = Message::builder().from(from).subject(&req.subject);

    for addr in &req.to {
        let mb: Mailbox = addr
            .parse()
            .map_err(|e: lettre::address::AddressError| SmtpError::Build(e.to_string()))?;
        builder = builder.to(mb);
    }
    for addr in &req.cc {
        let mb: Mailbox = addr
            .parse()
            .map_err(|e: lettre::address::AddressError| SmtpError::Build(e.to_string()))?;
        builder = builder.cc(mb);
    }
    for addr in &req.bcc {
        let mb: Mailbox = addr
            .parse()
            .map_err(|e: lettre::address::AddressError| SmtpError::Build(e.to_string()))?;
        builder = builder.bcc(mb);
    }

    if let Some(ref irt) = req.in_reply_to {
        builder = builder.in_reply_to(irt.clone());
    }
    if let Some(ref refs) = req.references {
        builder = builder.references(refs.clone());
    }

    // No attachments: simple body
    if req.attachments.is_empty() {
        return match (&req.body_text, &req.body_html) {
            (Some(text), Some(html)) => builder
                .multipart(
                    MultiPart::alternative()
                        .singlepart(SinglePart::plain(text.clone()))
                        .singlepart(SinglePart::html(html.clone())),
                )
                .map_err(|e| SmtpError::Build(e.to_string())),
            (Some(text), None) => builder
                .body(text.clone())
                .map_err(|e| SmtpError::Build(e.to_string())),
            (None, Some(html)) => builder
                .singlepart(SinglePart::html(html.clone()))
                .map_err(|e| SmtpError::Build(e.to_string())),
            (None, None) => builder
                .body(String::new())
                .map_err(|e| SmtpError::Build(e.to_string())),
        };
    }

    // With attachments
    let body_part = match (&req.body_text, &req.body_html) {
        (Some(text), Some(html)) => MultiPart::alternative()
            .singlepart(SinglePart::plain(text.clone()))
            .singlepart(SinglePart::html(html.clone())),
        (Some(text), None) => MultiPart::mixed().singlepart(SinglePart::plain(text.clone())),
        (None, Some(html)) => MultiPart::mixed().singlepart(SinglePart::html(html.clone())),
        (None, None) => MultiPart::mixed().singlepart(SinglePart::plain(String::new())),
    };

    let mut mixed = MultiPart::mixed().multipart(body_part);
    for att in &req.attachments {
        let data = base64::engine::general_purpose::STANDARD
            .decode(&att.data)
            .map_err(|e| SmtpError::Attachment(e.to_string()))?;
        let ct: ContentType = att
            .content_type
            .parse()
            .unwrap_or(ContentType::TEXT_PLAIN);
        mixed = mixed.singlepart(
            lettre::message::Attachment::new(att.filename.clone()).body(data, ct),
        );
    }

    builder
        .multipart(mixed)
        .map_err(|e| SmtpError::Build(e.to_string()))
}

fn build_transport(req: &SendRequest) -> Result<AsyncSmtpTransport<Tokio1Executor>, SmtpError> {
    let mut tls_builder = TlsParameters::builder(req.smtp_host.clone());
    if let Some(der) = &req.trusted_cert_der {
        let cert = lettre::transport::smtp::client::Certificate::from_der(der.clone())
            .map_err(|e| SmtpError::Build(format!("trusted certificate invalid: {e}")))?;
        tls_builder = tls_builder
            .add_root_certificate(cert)
            .dangerous_accept_invalid_hostnames(true);
    }
    let tls_params = tls_builder
        .build_native()
        .map_err(|e| SmtpError::Build(e.to_string()))?;

    let mut transport_builder =
        AsyncSmtpTransport::<Tokio1Executor>::relay(&req.smtp_host)
            .map_err(|e| SmtpError::Build(e.to_string()))?
            .port(req.smtp_port)
            .tls(Tls::Required(tls_params));

    transport_builder = match req.auth_scheme.as_str() {
        "xoauth2" => {
            let token = req.oauth_token.as_deref().unwrap_or("");
            let sasl = build_xoauth2_sasl(&req.smtp_user, token);
            transport_builder
                .credentials(Credentials::new(req.smtp_user.clone(), sasl))
                .authentication(vec![Mechanism::Plain])
        }
        _ => transport_builder.credentials(Credentials::new(
            req.smtp_user.clone(),
            req.smtp_pass.clone(),
        )),
    };

    Ok(transport_builder.build())
}

fn build_xoauth2_sasl(username: &str, token: &str) -> String {
    let raw = format!("user={username}\x01auth=Bearer {token}\x01\x01");
    base64::engine::general_purpose::STANDARD.encode(raw)
}
