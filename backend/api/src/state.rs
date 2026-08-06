use contact_sync::ContactSyncManager;
use db::pool::UserDbPool;
use mail_sync::manager::SyncManager;
use mailquill_core::{blob::BlobStore, crypto::CredentialKey, jwt::JwtKey};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct VapidConfig {
    pub public_key: String,
    pub private_key: String,
    pub subject: String,
}

/// A new-message event delivered to a user's open clients over SSE, enabling
/// foreground notifications when web push isn't available (e.g. Brave).
#[derive(Clone)]
pub struct UserEvent {
    pub user_id: String,
    /// SSE event name (`message`, `sync`, ...).
    pub event_type: String,
    /// JSON payload for the named event.
    pub payload: String,
}

#[derive(Clone)]
pub struct AppState {
    /// app.db — users, refresh_tokens, user_settings
    pub app_db: SqlitePool,
    /// per-user mail.db pool (email_accounts, messages, etc.)
    pub user_db_pool: Arc<UserDbPool>,
    /// blob store for message bodies and attachments
    pub blob_store: Arc<dyn BlobStore>,
    /// manages per-account IMAP sync tasks
    pub sync_manager: Arc<SyncManager>,
    /// manages per-account contact sync tasks
    pub contact_sync_manager: Arc<ContactSyncManager>,
    /// AES-256-GCM key for encrypting IMAP/SMTP credentials
    pub credential_key: Arc<CredentialKey>,
    /// JWT signing key
    pub jwt_key: Arc<JwtKey>,
    /// Web Push VAPID configuration, present only when env vars are configured.
    pub vapid: Option<Arc<VapidConfig>>,
    /// Reusable HTTP client for Web Push deliveries (requests are built by
    /// web-push-native and sent through reqwest).
    pub web_push_client: Option<reqwest::Client>,
    /// Broadcast of new-message events to connected SSE clients.
    pub events: broadcast::Sender<UserEvent>,
    /// Whether approved remote email images should be fetched through the backend.
    pub remote_image_proxy_enabled: bool,
}
