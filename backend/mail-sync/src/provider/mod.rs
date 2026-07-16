//! Provider abstraction for mailbox access.
//!
//! The sync pipeline (sync.rs), threading, and phishing analysis are already
//! transport-agnostic — this trait makes the mailbox I/O pluggable too. IMAP
//! is the baseline implementation. Gmail also has a REST API implementation;
//! Microsoft Graph is wired through the same abstraction but is enabled only
//! when its provider is selected for an account.
//!
//! Design notes for implementors:
//! - Folders are addressed by their provider-native path on every call; an
//!   implementation may cache a selected folder internally (IMAP does).
//! - `uid` is the provider-native per-folder message id. For Gmail/Graph the
//!   implementation must map its string ids onto stable u32s (or grow this
//!   type) — that mapping is the main open design question for those backends.

mod gmail;
mod gmail_imap;
mod http;
pub mod idmap;
mod imap;
mod outlook;
mod util;

pub use gmail::GmailProvider;
pub use gmail_imap::GmailImapProvider;
pub use idmap::IdMap;
pub use imap::ImapProvider;
pub use outlook::OutlookProvider;

use crate::session::{FetchedMessage, FolderInfo, SessionError};
use async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error("not implemented for this provider: {0}")]
    NotImplemented(&'static str),
    #[error("http {status}: {body}")]
    Http { status: u16, body: String },
    #[error("{0}")]
    Other(String),
}

/// Which backend an account talks to. Stored per account (currently every
/// account can choose the backend that should own sync and send behavior).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderKind {
    #[default]
    Imap,
    /// Gmail over IMAP/SMTP+XOAUTH2 for mail, Gmail API for label handling.
    GmailImap,
    GmailApi,
    OutlookApi,
}

impl ProviderKind {
    pub fn parse(value: &str) -> Self {
        match value {
            "gmail_imap" => Self::GmailImap,
            "gmail_api" => Self::GmailApi,
            "outlook_api" => Self::OutlookApi,
            _ => Self::Imap,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Imap => "imap",
            Self::GmailImap => "gmail_imap",
            Self::GmailApi => "gmail_api",
            Self::OutlookApi => "outlook_api",
        }
    }

    /// Mailbox is synced over IMAP (so IMAP IDLE applies). True for plain IMAP
    /// and the Gmail-over-IMAP hybrid.
    pub fn syncs_over_imap(&self) -> bool {
        matches!(self, Self::Imap | Self::GmailImap)
    }

    /// Outgoing mail goes through SMTP (vs. a provider send API).
    pub fn sends_over_smtp(&self) -> bool {
        matches!(self, Self::Imap | Self::GmailImap)
    }
}

/// Everything needed to open a connection, independent of the backend.
/// IMAP uses host/port/credentials; the API providers need the OAuth token
/// plus the user DB handle + account id for their uid↔remote-id mapping.
#[derive(Debug, Default, Clone)]
pub struct ProviderConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub oauth_access_token: Option<String>,
    pub auth_scheme: String,
    pub trusted_cert_der: Option<Vec<u8>>,
    /// User mail DB — required for Gmail/Outlook (remote_message_ids table).
    pub db: Option<sqlx::SqlitePool>,
    pub account_id: String,
}

/// Snapshot of a folder used by incremental sync.
#[derive(Debug, Clone, Copy)]
pub struct FolderStatus {
    /// Generation marker; when it changes, local cache for the folder is
    /// purged (IMAP UIDVALIDITY). API providers return a constant — their
    /// uid mapping is locally owned and never invalidated server-side.
    pub uidvalidity: u32,
    /// Number of messages in the folder as reported by the server.
    pub exists: u32,
}

/// Uniform mailbox operations. One instance = one live connection/session for
/// one account; not shared across accounts.
#[async_trait]
pub trait MailProvider: Send {
    /// All folders/labels with their provider-native path and a normalized
    /// folder type (INBOX/SENT/DRAFTS/ARCHIVE/SPAM/TRASH/OTHER).
    async fn list_folders(&mut self) -> Result<Vec<FolderInfo>, ProviderError>;

    /// Folder generation marker + message count (drives cache invalidation
    /// and sync progress totals).
    async fn folder_status(&mut self, folder: &str) -> Result<FolderStatus, ProviderError>;

    /// Highest message uid in a folder (None when empty). Drives incremental
    /// sync windows.
    async fn highest_uid(&mut self, folder: &str) -> Result<Option<u32>, ProviderError>;

    /// Read/flagged/deleted state for a uid set `(uid, seen, flagged, deleted)`,
    /// to reconcile already-synced messages.
    async fn fetch_flags(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<(u32, bool, bool, bool)>, ProviderError>;

    /// Envelope + raw header block per message (lazy sync).
    async fn fetch_headers(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<FetchedMessage>, ProviderError>;

    /// Complete raw RFC 2822 messages (full sync).
    async fn fetch_full(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<FetchedMessage>, ProviderError>;

    /// Complete raw RFC 2822 message for a single uid (on-demand body fetch).
    async fn fetch_raw(&mut self, folder: &str, uid: u32) -> Result<Vec<u8>, ProviderError>;

    /// Set/clear a normalized flag ("seen" / "flagged" / "deleted").
    async fn set_flag(
        &mut self,
        folder: &str,
        uid: u32,
        flag: &str,
        value: bool,
    ) -> Result<(), ProviderError>;

    /// Move a message between folders.
    async fn move_message(
        &mut self,
        src_folder: &str,
        uid: u32,
        dest_folder: &str,
    ) -> Result<(), ProviderError>;

    /// Permanently delete (IMAP: \Deleted + EXPUNGE).
    async fn delete_permanently(&mut self, folder: &str, uid: u32) -> Result<(), ProviderError>;

    /// Store a sent message in the provider's Sent folder. No-op for
    /// providers that do this server-side on send (Gmail API).
    async fn append_sent(&mut self, raw_message: &[u8]) -> Result<(), ProviderError>;

    /// Send a raw RFC 2822 message through the provider (Gmail/Graph send
    /// APIs). IMAP cannot send — those accounts deliver via SMTP instead.
    async fn send_message(&mut self, _raw_message: &[u8]) -> Result<(), ProviderError> {
        Err(ProviderError::NotImplemented("send"))
    }

    /// Graceful shutdown (IMAP LOGOUT). Best effort.
    async fn close(&mut self) -> Result<(), ProviderError>;
}

/// Open a connection for the given backend.
pub async fn connect(
    kind: ProviderKind,
    config: &ProviderConfig,
) -> Result<Box<dyn MailProvider>, ProviderError> {
    match kind {
        ProviderKind::Imap => Ok(Box::new(ImapProvider::connect(config).await?)),
        ProviderKind::GmailImap => Ok(Box::new(GmailImapProvider::connect(config).await?)),
        ProviderKind::GmailApi => Ok(Box::new(GmailProvider::new(config)?)),
        ProviderKind::OutlookApi => Ok(Box::new(OutlookProvider::new(config)?)),
    }
}
