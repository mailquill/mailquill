use db::pool::UserDbPool;
use mailquill_core::{blob::BlobStore, crypto::CredentialKey, jwt::JwtKey};
use imap_sync::manager::SyncManager;
use sqlx::SqlitePool;
use std::sync::Arc;
use web_push::IsahcWebPushClient;

#[derive(Clone)]
pub struct VapidConfig {
    pub public_key: String,
    pub private_key: String,
    pub subject: String,
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
    /// AES-256-GCM key for encrypting IMAP/SMTP credentials
    pub credential_key: Arc<CredentialKey>,
    /// JWT signing key
    pub jwt_key: Arc<JwtKey>,
    /// Web Push VAPID configuration, present only when env vars are configured.
    pub vapid: Option<Arc<VapidConfig>>,
    /// Reusable Web Push HTTP client.
    pub web_push_client: Option<Arc<IsahcWebPushClient>>,
}
