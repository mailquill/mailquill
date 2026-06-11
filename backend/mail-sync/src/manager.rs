use std::{collections::HashMap, sync::Arc};
use tokio::{
    sync::{mpsc, Mutex},
    task::JoinHandle,
};

/// Status for a single account's sync task.
#[derive(Clone, Debug)]
pub struct SyncStatus {
    pub state: String,
    pub last_synced_at: Option<String>,
    pub error: Option<String>,
    /// Messages persisted so far in the current/last sync run.
    pub synced: i64,
    /// Total messages reported by the server across all folders.
    pub total: i64,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            last_synced_at: None,
            error: None,
            synced: 0,
            total: 0,
        }
    }
}

/// Command sent to the sync task for an account.
pub enum SyncCommand {
    Shutdown,
    ForcePoll,
    IMapMove {
        user_id: String,
        uid: u32,
        src_folder: String,
        dest_folder: String,
        expunge: bool,
    },
    IMapFlag {
        user_id: String,
        uid: u32,
        folder: String,
        flag: String,
        set: bool,
    },
    IMapExpunge {
        user_id: String,
        uid: u32,
        folder: String,
    },
}

struct AccountTask {
    tx: mpsc::Sender<SyncCommand>,
    handle: JoinHandle<()>,
}

pub struct SyncManager {
    tasks: Mutex<HashMap<String, AccountTask>>,
    statuses: Mutex<HashMap<String, SyncStatus>>,
}

impl SyncManager {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
            statuses: Mutex::new(HashMap::new()),
        }
    }

    /// Start a full sync task for an account (called after account creation).
    pub async fn start_account(
        &self,
        account_id: String,
        user_id: String,
        app_state: Arc<dyn SyncAppState>,
    ) {
        let (tx, rx) = mpsc::channel(32);
        let account_id_clone = account_id.clone();
        let user_id_clone = user_id.clone();

        let handle = tokio::spawn(async move {
            crate::sync::run_sync_task(account_id_clone, user_id_clone, rx, app_state).await;
        });

        let mut tasks = self.tasks.lock().await;
        // Stop any existing task for this account
        if let Some(old) = tasks.remove(&account_id) {
            let _ = old.tx.send(SyncCommand::Shutdown).await;
            old.handle.abort();
        }
        tasks.insert(account_id.clone(), AccountTask { tx, handle });

        self.statuses
            .lock()
            .await
            .insert(account_id, SyncStatus::default());
    }

    /// Start sync with minimal context (used at server restart).
    pub async fn start_account_minimal(&self, account_id: String, _user_id: String) {
        // Mark as needing sync — actual sync started lazily on first request or separately
        self.statuses.lock().await.insert(
            account_id,
            SyncStatus {
                state: "pending".into(),
                ..Default::default()
            },
        );
    }

    /// Stop the sync task for an account.
    pub async fn stop_account(&self, account_id: &str) {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.remove(account_id) {
            let _ = task.tx.send(SyncCommand::Shutdown).await;
            task.handle.abort();
        }
        self.statuses.lock().await.remove(account_id);
    }

    pub async fn account_status(&self, account_id: &str) -> SyncStatus {
        self.statuses
            .lock()
            .await
            .get(account_id)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn update_status(&self, account_id: &str, status: SyncStatus) {
        self.statuses
            .lock()
            .await
            .insert(account_id.to_owned(), status);
    }

    /// Update lifecycle fields (state/last_synced_at/error) while preserving the
    /// current progress counters.
    pub async fn set_state(
        &self,
        account_id: &str,
        state: &str,
        last_synced_at: Option<String>,
        error: Option<String>,
    ) {
        let mut statuses = self.statuses.lock().await;
        let st = statuses.entry(account_id.to_owned()).or_default();
        st.state = state.to_owned();
        st.last_synced_at = last_synced_at;
        st.error = error;
    }

    /// Update progress counters while preserving lifecycle fields.
    pub async fn set_progress(&self, account_id: &str, synced: i64, total: i64) {
        let mut statuses = self.statuses.lock().await;
        let st = statuses.entry(account_id.to_owned()).or_default();
        st.synced = synced;
        st.total = total;
    }

    /// Queue an immediate sync poll for an account.
    pub async fn force_poll(&self, account_id: &str) -> bool {
        let tasks = self.tasks.lock().await;
        if let Some(task) = tasks.get(account_id) {
            task.tx.send(SyncCommand::ForcePoll).await.is_ok()
        } else {
            false
        }
    }

    /// Queue an IMAP MOVE operation.
    pub async fn queue_imap_move(
        &self,
        user_id: String,
        account_id: String,
        uid: u32,
        src_folder: String,
        dest_folder: String,
        expunge: bool,
    ) {
        let tasks = self.tasks.lock().await;
        if let Some(task) = tasks.get(&account_id) {
            let _ = task
                .tx
                .send(SyncCommand::IMapMove {
                    user_id,
                    uid,
                    src_folder,
                    dest_folder,
                    expunge,
                })
                .await;
        }
    }

    /// Queue an IMAP flag change.
    pub async fn queue_imap_flag(
        &self,
        user_id: String,
        account_id: String,
        uid: u32,
        folder: String,
        flag: String,
        set: bool,
    ) {
        let tasks = self.tasks.lock().await;
        if let Some(task) = tasks.get(&account_id) {
            let _ = task
                .tx
                .send(SyncCommand::IMapFlag {
                    user_id,
                    uid,
                    folder,
                    flag,
                    set,
                })
                .await;
        }
    }

    /// Queue an IMAP EXPUNGE.
    pub async fn queue_imap_expunge(
        &self,
        user_id: String,
        account_id: String,
        uid: u32,
        folder: String,
    ) {
        let tasks = self.tasks.lock().await;
        if let Some(task) = tasks.get(&account_id) {
            let _ = task
                .tx
                .send(SyncCommand::IMapExpunge {
                    user_id,
                    uid,
                    folder,
                })
                .await;
        }
    }
}

#[derive(Clone, Debug)]
pub struct NewMessageNotification {
    pub message_id: String,
    pub account_id: String,
    pub account_name: String,
    pub sender: String,
    pub subject: String,
}

/// Trait implemented by AppState so mail-sync doesn't depend on api types.
#[async_trait::async_trait]
pub trait SyncAppState: Send + Sync + 'static {
    async fn user_db(&self, user_id: &str) -> Result<sqlx::SqlitePool, String>;
    fn blob_store(&self) -> Arc<dyn mailquill_core::blob::BlobStore>;
    fn credential_key(&self) -> Arc<mailquill_core::crypto::CredentialKey>;
    fn sync_manager(&self) -> Arc<SyncManager>;
    async fn notify_new_message(
        &self,
        _user_id: &str,
        _message: NewMessageNotification,
    ) -> Result<(), String> {
        Ok(())
    }

    /// A currently valid OAuth access token for the account, refreshed if
    /// necessary. `None` for accounts without OAuth credentials. The api
    /// crate implements the actual refresh (it owns the client secrets).
    async fn fresh_oauth_token(&self, _user_id: &str, _account_id: &str) -> Option<String> {
        None
    }
}
