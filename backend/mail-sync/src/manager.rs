use serde::Serialize;
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
    /// Push every flag change queued in `pending_flag_ops` to the server.
    /// Carries no payload: the ops live in the user database, so a coalesced
    /// wake-up is enough and nothing is lost if this signal is dropped.
    FlushFlags { user_id: String },
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
        // The task gets its own sender so the IMAP IDLE child can queue polls.
        let self_tx = tx.clone();

        let mut tasks = self.tasks.lock().await;
        // Stop any existing task for this account
        if let Some(old) = tasks.remove(&account_id) {
            let _ = old.tx.try_send(SyncCommand::Shutdown);
            old.handle.abort();
        }
        self.statuses
            .lock()
            .await
            .insert(account_id.clone(), SyncStatus::default());

        let handle = tokio::spawn(async move {
            crate::sync::run_sync_task(account_id_clone, user_id_clone, rx, self_tx, app_state)
                .await;
        });
        tasks.insert(account_id, AccountTask { tx, handle });
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
            let _ = task.tx.try_send(SyncCommand::Shutdown);
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

    /// The command sender of an account's sync task, if one is running.
    ///
    /// Callers send on the returned clone after the registry lock is released:
    /// the channel is bounded and a task busy with a long sync can keep it full,
    /// and waiting for capacity while holding the lock would stall every other
    /// account's commands with it.
    async fn task_sender(&self, account_id: &str) -> Option<mpsc::Sender<SyncCommand>> {
        self.tasks
            .lock()
            .await
            .get(account_id)
            .map(|task| task.tx.clone())
    }

    /// Queue an immediate sync poll for an account.
    ///
    /// Returns whether the account has a live sync task. Never waits: a full
    /// channel means the task is alive with work queued, and its ticker polls
    /// anyway, so dropping this poll is fine — blocking the caller (an HTTP
    /// request) until a long sync drains the queue is not.
    pub async fn force_poll(&self, account_id: &str) -> bool {
        match self.task_sender(account_id).await {
            Some(tx) => !matches!(
                tx.try_send(SyncCommand::ForcePoll),
                Err(mpsc::error::TrySendError::Closed(_))
            ),
            None => false,
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
        if let Some(tx) = self.task_sender(&account_id).await {
            let _ = tx
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

    /// Ask an account's sync task to push its pending flag changes now.
    ///
    /// Best-effort by design: the ops are already persisted, so a full channel
    /// (a flush is queued anyway) or a missing task (account paused, task
    /// restarting) only delays the push to the next sync cycle instead of
    /// losing it.
    pub async fn queue_flag_flush(&self, user_id: String, account_id: &str) {
        if let Some(tx) = self.task_sender(account_id).await {
            let _ = tx.try_send(SyncCommand::FlushFlags { user_id });
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
        if let Some(tx) = self.task_sender(&account_id).await {
            let _ = tx
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

#[derive(Clone, Debug, Serialize)]
pub struct SyncStatusNotification {
    pub account_id: String,
    pub state: String,
    pub last_synced_at: Option<String>,
    pub error: Option<String>,
    pub synced: i64,
    pub total: i64,
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

    async fn notify_sync_status(
        &self,
        _user_id: &str,
        _status: SyncStatusNotification,
    ) -> Result<(), String> {
        Ok(())
    }

    /// A currently valid OAuth access token for the account, refreshed if
    /// necessary. `None` for accounts without OAuth credentials. The api
    /// crate implements the actual refresh (it owns the client secrets).
    async fn fresh_oauth_token(
        &self,
        _user_id: &str,
        _account_id: &str,
    ) -> Result<Option<String>, String> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Registers an account whose command channel is already full, like a sync
    /// task stuck in a long cycle. The receiver is kept so the channel stays open.
    async fn busy_account(manager: &SyncManager) -> mpsc::Receiver<SyncCommand> {
        let (tx, rx) = mpsc::channel(1);
        tx.try_send(SyncCommand::ForcePoll).unwrap();
        let handle = tokio::spawn(async {});
        manager
            .tasks
            .lock()
            .await
            .insert("busy".into(), AccountTask { tx, handle });
        rx
    }

    #[tokio::test]
    async fn force_poll_on_a_busy_task_reports_it_alive_without_waiting() {
        let manager = SyncManager::new();
        let _rx = busy_account(&manager).await;

        let alive = tokio::time::timeout(Duration::from_secs(1), manager.force_poll("busy"))
            .await
            .expect("force_poll must not wait for channel capacity");
        assert!(alive, "a full channel still means the task is running");
    }

    #[tokio::test]
    async fn a_busy_account_does_not_block_commands_for_other_accounts() {
        let manager = Arc::new(SyncManager::new());
        let _rx = busy_account(&manager).await;

        // Waits for capacity on the busy account's channel.
        let blocked = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager
                    .queue_imap_expunge("user".into(), "busy".into(), 1, "Trash".into())
                    .await;
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;

        let (tx, mut rx) = mpsc::channel(4);
        let handle = tokio::spawn(async {});
        tokio::time::timeout(Duration::from_secs(1), async {
            manager
                .tasks
                .lock()
                .await
                .insert("other".into(), AccountTask { tx, handle });
            manager
                .queue_imap_expunge("user".into(), "other".into(), 2, "Trash".into())
                .await;
        })
        .await
        .expect("the registry lock must not be held while waiting on a full channel");
        assert!(matches!(
            rx.try_recv(),
            Ok(SyncCommand::IMapExpunge { uid: 2, .. })
        ));
        blocked.abort();
    }
}
