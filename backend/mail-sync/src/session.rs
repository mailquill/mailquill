use async_imap::{error::Error as ImapError, Authenticator, Session};
use base64::Engine;
use futures::TryStreamExt;
use native_tls::TlsConnector;
use tokio::net::TcpStream;
use tokio_native_tls::TlsStream;

pub type ImapSession = Session<TlsStream<TcpStream>>;

#[derive(Debug)]
pub struct FolderInfo {
    pub name: String,
    pub full_path: String,
    pub folder_type: String,
}

#[derive(Debug, Default)]
pub struct FetchedMessage {
    pub uid: u32,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    pub list_id: Option<String>,
    pub subject: String,
    pub from_addr: String,
    pub to_addrs: String,
    pub cc_addrs: String,
    pub date: Option<String>,
    pub internal_date: String,
    pub is_seen: bool,
    pub is_flagged: bool,
    /// Server has the `\Deleted` flag set (awaiting EXPUNGE) — import as hidden
    /// so a message moved-by-copy doesn't linger as a live duplicate.
    pub is_deleted: bool,
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("IMAP error: {0}")]
    Imap(#[from] ImapError),
    #[error("TLS error: {0}")]
    Tls(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("auth error: {0}")]
    Auth(String),
    #[error("{0}")]
    Other(String),
}

impl From<tokio_native_tls::native_tls::Error> for SessionError {
    fn from(e: tokio_native_tls::native_tls::Error) -> Self {
        SessionError::Tls(e.to_string())
    }
}

struct XOAuth2Authenticator {
    sasl: String,
}

impl Authenticator for XOAuth2Authenticator {
    type Response = String;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        self.sasl.clone()
    }
}

/// Decode a stored trust-exception certificate (base64 DER, as kept in
/// `email_accounts.imap_tls_cert`/`smtp_tls_cert`). Invalid values are treated
/// as absent so a corrupt row degrades to strict verification, never weaker.
pub fn decode_trusted_cert(b64: Option<&str>) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(b64?.trim())
        .ok()
}

/// Open TLS IMAP session.
///
/// `trusted_cert_der` is a user-approved trust exception (DER certificate
/// accepted in the account wizard): it is added as a trust anchor and
/// hostname verification is skipped, mirroring Thunderbird's security
/// exceptions. `None` keeps strict WebPKI verification.
pub async fn connect_imap(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    oauth_token: Option<&str>,
    auth_scheme: &str,
    trusted_cert_der: Option<&[u8]>,
) -> Result<ImapSession, SessionError> {
    let mut builder = TlsConnector::builder();
    if let Some(der) = trusted_cert_der {
        let cert = native_tls::Certificate::from_der(der)
            .map_err(|e| SessionError::Tls(format!("trusted certificate invalid: {e}")))?;
        builder
            .add_root_certificate(cert)
            .danger_accept_invalid_hostnames(true);
    }
    let tls = builder
        .build()
        .map_err(|e| SessionError::Tls(e.to_string()))?;
    let tls = tokio_native_tls::TlsConnector::from(tls);

    let addr = format!("{host}:{port}");
    let stream = TcpStream::connect(&addr).await?;
    let tls_stream = tls.connect(host, stream).await?;

    let mut client = async_imap::Client::new(tls_stream);
    // consume server greeting
    client.read_response().await;

    let session = match auth_scheme {
        "xoauth2" => {
            let token = oauth_token
                .ok_or_else(|| SessionError::Auth("XOAUTH2 requires oauth_token".into()))?;
            let sasl = build_xoauth2_sasl(username, token);
            client
                .authenticate("XOAUTH2", XOAuth2Authenticator { sasl })
                .await
                .map_err(|(e, _)| SessionError::Auth(e.to_string()))?
        }
        _ => client
            .login(username, password)
            .await
            .map_err(|(e, _)| SessionError::Auth(e.to_string()))?,
    };

    Ok(session)
}

/// Run one IMAP IDLE cycle on the currently selected folder. Returns the
/// session (IDLE is left via DONE so it stays usable) plus whether the server
/// reported activity — new mail, a flag change, an expunge. A timeout returns
/// `false`. IDLE must be refreshed at least every 29 min (RFC 2177), so keep
/// `max_wait` under that and re-issue in a loop.
pub async fn idle_once(
    session: ImapSession,
    max_wait: std::time::Duration,
) -> Result<(ImapSession, bool), SessionError> {
    use async_imap::extensions::idle::IdleResponse;

    let mut handle = session.idle();
    handle.init().await?;
    let activity = {
        // `wait` borrows `handle`; `_interrupt` stays alive until the borrow ends.
        let (wait, _interrupt) = handle.wait_with_timeout(max_wait);
        matches!(wait.await?, IdleResponse::NewData(_))
    };
    let session = handle.done().await?;
    Ok((session, activity))
}

/// Build XOAUTH2 SASL string: base64("user=<user>\x01auth=Bearer <token>\x01\x01")
pub fn build_xoauth2_sasl(username: &str, token: &str) -> String {
    let raw = format!("user={username}\x01auth=Bearer {token}\x01\x01");
    base64::engine::general_purpose::STANDARD.encode(raw)
}

/// List all IMAP folders.
pub async fn list_folders(session: &mut ImapSession) -> Result<Vec<FolderInfo>, SessionError> {
    let mailboxes: Vec<_> = session.list(None, Some("*")).await?.try_collect().await?;
    let mut folders = Vec::new();
    for mb in &mailboxes {
        let name = mb.name().to_string();
        // Prefer the server's RFC 6154 SPECIAL-USE attributes (\Trash, \Junk,
        // \Archive, …) so localized folders like "Papierkorb" still classify
        // correctly. Fall back to name heuristics when none are advertised.
        let folder_type = special_use_type(mb.attributes())
            .map(str::to_owned)
            .unwrap_or_else(|| classify_folder(&name));
        folders.push(FolderInfo {
            name: name.clone(),
            full_path: name,
            folder_type,
        });
    }
    Ok(folders)
}

/// Map an RFC 6154 SPECIAL-USE attribute to our folder_type, if present.
fn special_use_type(attrs: &[async_imap::types::NameAttribute<'_>]) -> Option<&'static str> {
    use async_imap::types::NameAttribute as A;
    attrs.iter().find_map(|a| match a {
        A::Trash => Some("TRASH"),
        A::Junk => Some("SPAM"),
        A::Archive => Some("ARCHIVE"),
        // Gmail has no \Archive; "All Mail" (\All) is where archived mail lives,
        // so treat it as the archive target. Harmless on non-Gmail servers,
        // which don't advertise \All.
        A::All => Some("ARCHIVE"),
        A::Sent => Some("SENT"),
        A::Drafts => Some("DRAFTS"),
        _ => None,
    })
}

fn classify_folder(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower == "inbox" {
        "INBOX"
    } else if lower.contains("sent") {
        "SENT"
    } else if lower.contains("draft") {
        "DRAFTS"
    } else if lower.contains("trash") || lower.contains("deleted") {
        "TRASH"
    } else if lower.contains("spam") || lower.contains("junk") {
        "SPAM"
    } else if lower.contains("archive") {
        "ARCHIVE"
    } else {
        "CUSTOM"
    }
    .into()
}

/// Select a folder and return (UIDVALIDITY, EXISTS).
pub async fn select_folder(
    session: &mut ImapSession,
    folder: &str,
) -> Result<(u32, u32), SessionError> {
    let mb = session.select(folder).await?;
    let uidvalidity = mb.uid_validity.unwrap_or(0);
    let exists = mb.exists;
    Ok((uidvalidity, exists))
}

/// Check if the server has THREAD=REFERENCES capability.
pub async fn check_thread_capability(session: &mut ImapSession) -> bool {
    match session.capabilities().await {
        Ok(caps) => caps.has_str("THREAD=REFERENCES"),
        Err(_) => false,
    }
}

/// Highest UID in the currently selected mailbox, or `None` if it is empty.
///
/// `UID FETCH * (UID)` targets the message with the highest sequence number;
/// because UIDs increase monotonically with sequence, its UID is the largest.
/// Used to bound chunked backfill instead of fetching an open-ended `n:*` range.
pub async fn highest_uid(session: &mut ImapSession) -> Result<Option<u32>, SessionError> {
    let msgs: Vec<_> = session.uid_fetch("*", "(UID)").await?.try_collect().await?;
    Ok(msgs.iter().filter_map(|m| m.uid).max())
}

/// Fetch only FLAGS for a UID range, returning `(uid, is_seen, is_flagged,
/// is_deleted)`. Used to reconcile read/flagged/deleted state of already-synced
/// messages with the server, since header backfill only covers new UIDs.
pub async fn fetch_flags(
    session: &mut ImapSession,
    uid_set: &str,
) -> Result<Vec<(u32, bool, bool, bool)>, SessionError> {
    let msgs: Vec<_> = session
        .uid_fetch(uid_set, "(UID FLAGS)")
        .await?
        .try_collect()
        .await?;

    let mut out = Vec::new();
    for m in &msgs {
        if let Some(uid) = m.uid {
            let seen = m
                .flags()
                .any(|f| matches!(f, async_imap::types::Flag::Seen));
            let flagged = m
                .flags()
                .any(|f| matches!(f, async_imap::types::Flag::Flagged));
            let deleted = m
                .flags()
                .any(|f| matches!(f, async_imap::types::Flag::Deleted));
            out.push((uid, seen, flagged, deleted));
        }
    }
    Ok(out)
}

/// Fetch the Gmail `X-GM-MSGID` extension for a uid set, returning
/// `(uid, gmail_message_id_hex)`. The hex form is exactly the Gmail API
/// message id. Issued as a raw command because async-imap's typed `Fetch`
/// doesn't expose the Gmail extension attributes; the response is fully drained
/// up to the tagged completion so the session stays usable afterwards.
pub async fn fetch_gmail_msgids(
    session: &mut ImapSession,
    uid_set: &str,
) -> Result<Vec<(u32, String)>, SessionError> {
    use async_imap::imap_proto::{types::AttributeValue, Response};

    let id = session
        .run_command(format!("UID FETCH {uid_set} (UID X-GM-MSGID)"))
        .await?;
    let mut out = Vec::new();
    while let Some(resp) = session.read_response().await {
        let resp = resp?;
        match resp.parsed() {
            Response::Fetch(_, attrs) => {
                let mut uid = None;
                let mut gm = None;
                for a in attrs {
                    match a {
                        AttributeValue::Uid(u) => uid = Some(*u),
                        AttributeValue::GmailMsgId(g) => gm = Some(*g),
                        _ => {}
                    }
                }
                if let (Some(u), Some(g)) = (uid, gm) {
                    out.push((u, format!("{g:x}")));
                }
            }
            Response::Done { tag, .. } if *tag == id => break,
            _ => {}
        }
    }
    Ok(out)
}

/// Fetch headers only (lazy sync).
pub async fn fetch_headers(
    session: &mut ImapSession,
    uid_set: &str,
) -> Result<Vec<FetchedMessage>, SessionError> {
    // Items MUST be parenthesised: async-imap forwards the query verbatim, and a
    // bare multi-item list (`UID FLAGS …`) is invalid IMAP, so the server returns
    // nothing. `UID` is included so `Fetch::uid` is populated for `parse_fetch`.
    let msgs: Vec<_> = session
        .uid_fetch(
            uid_set,
            "(UID FLAGS ENVELOPE INTERNALDATE BODY.PEEK[HEADER])",
        )
        .await?
        .try_collect()
        .await?;

    let mut messages = Vec::new();
    for msg in &msgs {
        if let Some(m) = parse_fetch(msg, false) {
            messages.push(m);
        }
    }
    Ok(messages)
}

/// Fetch full RFC822 messages (full sync).
pub async fn fetch_full(
    session: &mut ImapSession,
    uid_set: &str,
) -> Result<Vec<FetchedMessage>, SessionError> {
    // See `fetch_headers`: items must be parenthesised, and UID requested so
    // `Fetch::uid` is populated.
    let msgs: Vec<_> = session
        .uid_fetch(uid_set, "(UID FLAGS INTERNALDATE RFC822)")
        .await?
        .try_collect()
        .await?;

    let mut messages = Vec::new();
    for msg in &msgs {
        if let Some(m) = parse_fetch(msg, true) {
            messages.push(m);
        }
    }
    Ok(messages)
}

fn parse_fetch(msg: &async_imap::types::Fetch, include_body: bool) -> Option<FetchedMessage> {
    let uid = msg.uid?;

    let internal_date = msg
        .internal_date()
        .map(|d| d.to_rfc3339())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let is_seen = msg
        .flags()
        .any(|f| matches!(f, async_imap::types::Flag::Seen));
    let is_flagged = msg
        .flags()
        .any(|f| matches!(f, async_imap::types::Flag::Flagged));
    let is_deleted = msg
        .flags()
        .any(|f| matches!(f, async_imap::types::Flag::Deleted));

    let mut fetched = FetchedMessage {
        uid,
        internal_date,
        is_seen,
        is_flagged,
        is_deleted,
        ..Default::default()
    };

    if let Some(env) = msg.envelope() {
        if let Some(subject) = env.subject.as_ref() {
            fetched.subject = decode_words(&bytes_to_string(subject));
        }
        if let Some(from) = env.from.as_ref().and_then(|v| v.first()) {
            fetched.from_addr = format_address(from);
        }
        if let Some(to) = env.to.as_ref() {
            fetched.to_addrs = to.iter().map(format_address).collect::<Vec<_>>().join(", ");
        }
        if let Some(cc) = env.cc.as_ref() {
            fetched.cc_addrs = cc.iter().map(format_address).collect::<Vec<_>>().join(", ");
        }
        if let Some(date) = env.date.as_ref() {
            fetched.date = Some(bytes_to_string(date));
        }
        if let Some(mid) = env.message_id.as_ref() {
            fetched.message_id = Some(bytes_to_string(mid));
        }
        if let Some(irt) = env.in_reply_to.as_ref() {
            fetched.in_reply_to = Some(bytes_to_string(irt));
        }
    }

    // Extract References and List-Id from raw headers
    if let Some(header_bytes) = msg.header() {
        if let Ok((headers, _)) = mailparse::parse_headers(header_bytes) {
            use mailparse::MailHeaderMap;
            if let Some(v) = headers.get_first_value("References") {
                fetched.references = Some(v);
            }
            if let Some(v) = headers.get_first_value("List-Id") {
                fetched.list_id = Some(v);
            }
            if fetched.subject.is_empty() {
                if let Some(v) = headers.get_first_value("Subject") {
                    fetched.subject = v;
                }
            }
        }
    }

    if include_body {
        fetched.body = msg.body().map(|b| b.to_vec());
    } else {
        // Header-only fetch (lazy sync): keep the raw header block so
        // sync-time phishing analysis can inspect Authentication-Results,
        // Reply-To, and Return-Path. parse_mime on it yields no body parts,
        // so snippet/blob handling is unaffected.
        fetched.body = msg.header().map(|b| b.to_vec());
    }

    Some(fetched)
}

fn format_address(addr: &async_imap::imap_proto::types::Address) -> String {
    let mailbox = addr
        .mailbox
        .as_ref()
        .map(|b| bytes_to_string(b))
        .unwrap_or_default();
    let host = addr
        .host
        .as_ref()
        .map(|b| bytes_to_string(b))
        .unwrap_or_default();
    let name = addr
        .name
        .as_ref()
        .map(|b| decode_words(&bytes_to_string(b)))
        .unwrap_or_default();

    if name.is_empty() {
        format!("{mailbox}@{host}")
    } else {
        format!("{name} <{mailbox}@{host}>")
    }
}

/// Decode RFC 2047 encoded-words (e.g. `=?UTF-8?B?...?=`) in a header value.
/// IMAP ENVELOPE returns raw header text, so subjects and display names arrive
/// still encoded; mailparse decodes encoded-words via `get_value`.
fn decode_words(value: &str) -> String {
    if !value.contains("=?") {
        return value.to_owned();
    }
    match mailparse::parse_header(format!("X: {value}").as_bytes()) {
        Ok((h, _)) => h.get_value(),
        Err(_) => value.to_owned(),
    }
}

fn bytes_to_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// MOVE a message to another folder.
pub async fn move_message(
    session: &mut ImapSession,
    uid: u32,
    dest_folder: &str,
    expunge_after: bool,
) -> Result<(), SessionError> {
    let uid_set = uid.to_string();
    let result = session.uid_mv(&uid_set, dest_folder).await;

    if result.is_err() {
        // Fallback: COPY + mark deleted + expunge
        session.uid_copy(&uid_set, dest_folder).await?;
        session
            .uid_store(&uid_set, "+FLAGS.SILENT (\\Deleted)")
            .await?
            .try_collect::<Vec<_>>()
            .await?;
        if expunge_after {
            session.expunge().await?.try_collect::<Vec<_>>().await?;
        }
    }
    Ok(())
}

/// Set or clear an IMAP flag.
pub async fn set_flag(
    session: &mut ImapSession,
    uid: u32,
    flag: &str,
    set: bool,
) -> Result<(), SessionError> {
    let imap_flag = match flag {
        "seen" => "\\Seen",
        "flagged" => "\\Flagged",
        "deleted" => "\\Deleted",
        other => other,
    };
    let op = if set {
        "+FLAGS.SILENT"
    } else {
        "-FLAGS.SILENT"
    };
    session
        .uid_store(&uid.to_string(), &format!("{op} ({imap_flag})"))
        .await?
        .try_collect::<Vec<_>>()
        .await?;
    Ok(())
}

/// EXPUNGE a specific UID (mark deleted + expunge).
pub async fn expunge_uid(session: &mut ImapSession, uid: u32) -> Result<(), SessionError> {
    let uid_set = uid.to_string();
    session
        .uid_store(&uid_set, "+FLAGS.SILENT (\\Deleted)")
        .await?
        .try_collect::<Vec<_>>()
        .await?;
    session.expunge().await?.try_collect::<Vec<_>>().await?;
    Ok(())
}

/// Fetch a single message body by UID.
pub async fn fetch_body_uid(session: &mut ImapSession, uid: u32) -> Result<Vec<u8>, SessionError> {
    let msgs: Vec<_> = session
        .uid_fetch(&uid.to_string(), "BODY[]")
        .await?
        .try_collect()
        .await?;
    for msg in &msgs {
        if let Some(body) = msg.body() {
            return Ok(body.to_vec());
        }
    }
    Err(SessionError::Other(format!("no body for uid {uid}")))
}

/// Append a message to the Sent folder.
pub async fn append_to_sent(
    session: &mut ImapSession,
    raw_message: &[u8],
) -> Result<(), SessionError> {
    let mailboxes: Vec<_> = session.list(None, Some("*")).await?.try_collect().await?;
    let sent_folder = mailboxes
        .iter()
        .find(|mb| mb.name().to_lowercase().contains("sent"))
        .map(|mb| mb.name().to_string());

    if let Some(folder) = sent_folder {
        session
            .append(&folder, Some("(\\Seen)"), None, raw_message)
            .await?;
    }
    Ok(())
}
