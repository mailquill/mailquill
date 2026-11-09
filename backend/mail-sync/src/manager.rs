use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use tokio::{
    sync::{mpsc, Mutex},
    task::JoinHandle,
};

/// Status for a single account's sync task.
#[derive(Clone, Debug)]
pub struct SyncStatus {
    /// Lifecycle state: `idle`, `syncing`, `error` or `reauth_required`.
    pub state: String,
    /// RFC 3339 time of the last successfully completed sync cycle, if any.
    pub last_synced_at: Option<String>,
    /// Message of the error that ended the last cycle, cleared on success.
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
    /// Stop the task after the current cycle.
    Shutdown,
    /// Run a sync cycle now instead of waiting for the next tick.
    ForcePoll,
    /// Push every flag change queued in `pending_flag_ops` to the server.
    /// Carries no payload: the ops live in the user database, so a coalesced
    /// wake-up is enough and nothing is lost if this signal is dropped.
    FlushFlags {
        /// Owner of the account; selects the user database.
        user_id: String,
    },
    /// Push every move/expunge queued in `pending_message_ops` to the server.
    /// Like [`Self::FlushFlags`] it carries no payload: the operations live in
    /// the user database and a coalesced wake-up is enough.
    FlushMessageOps {
        /// Owner of the account; selects the user database.
        user_id: String,
    },
}

struct AccountTask {
    tx: mpsc::Sender<SyncCommand>,
    handle: JoinHandle<()>,
}

/// Registry of the running per-account sync tasks and their statuses.
///
/// Each account gets one Tokio task fed through a bounded command channel.
/// The `queue_*` methods hand commands off without waiting for channel
/// capacity, so HTTP handlers are never stalled by a task busy with a long
/// sync cycle. Shared via `Arc` through [`SyncAppState::sync_manager`].
pub struct SyncManager {
    tasks: Mutex<HashMap<String, AccountTask>>,
    statuses: Mutex<HashMap<String, SyncStatus>>,
}

impl SyncManager {
    /// Create an empty registry with no running tasks.
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

    /// Stop the sync task for an account.
    pub async fn stop_account(&self, account_id: &str) {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.remove(account_id) {
            let _ = task.tx.try_send(SyncCommand::Shutdown);
            task.handle.abort();
        }
        self.statuses.lock().await.remove(account_id);
    }

    /// Current status of an account; the default (`idle`) for unknown accounts.
    pub async fn account_status(&self, account_id: &str) -> SyncStatus {
        self.statuses
            .lock()
            .await
            .get(account_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Replace an account's whole status (see [`Self::set_state`] and
    /// [`Self::set_progress`] for partial updates).
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

    /// Hand a command to a sync task without waiting for channel capacity.
    ///
    /// A full channel means the task is busy with a long cycle; the send then
    /// completes in the background once the task drains the queue.
    fn deliver(tx: mpsc::Sender<SyncCommand>, command: SyncCommand) {
        match tx.try_send(command) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(command)) => {
                tokio::spawn(async move {
                    let _ = tx.send(command).await;
                });
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {}
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

    /// Ask an account's sync task to push its pending moves/expunges now.
    ///
    /// The operations are already persisted in `pending_message_ops`, so this
    /// only has to wake the task. Unlike [`Self::queue_flag_flush`] the nudge
    /// is delivered even while the channel is full (a background send takes
    /// over): the user just hid the rows and expects the server to follow
    /// without waiting for the next periodic cycle. A missing task (account
    /// paused, task restarting) leaves the ops for the next sync cycle.
    pub async fn queue_message_op_flush(&self, user_id: String, account_id: &str) {
        if let Some(tx) = self.task_sender(account_id).await {
            Self::deliver(tx, SyncCommand::FlushMessageOps { user_id });
        }
    }
}

/// A newly synced message worth telling the user about (SSE + web push).
#[derive(Clone, Debug)]
pub struct NewMessageNotification {
    /// Local database id of the message row.
    pub message_id: String,
    /// Account that received the message.
    pub account_id: String,
    /// Display name of the account, for the notification body.
    pub account_name: String,
    /// Formatted `From` address.
    pub sender: String,
    /// Decoded subject line.
    pub subject: String,
}

/// Serialised snapshot of a [`SyncStatus`] pushed to clients over SSE.
#[derive(Clone, Debug, Serialize)]
pub struct SyncStatusNotification {
    /// Account the status belongs to.
    pub account_id: String,
    /// Lifecycle state, as in [`SyncStatus::state`].
    pub state: String,
    /// RFC 3339 time of the last successful sync, if any.
    pub last_synced_at: Option<String>,
    /// Error message of the last failed cycle, if any.
    pub error: Option<String>,
    /// Messages persisted so far in the current/last run.
    pub synced: i64,
    /// Total messages reported by the server across all folders.
    pub total: i64,
}

/// Trait implemented by AppState so mail-sync doesn't depend on api types.
#[async_trait::async_trait]
pub trait SyncAppState: Send + Sync + 'static {
    /// Open (or fetch the pooled) SQLite database of the given user.
    async fn user_db(&self, user_id: &str) -> Result<sqlx::SqlitePool, String>;
    /// Store for message bodies and attachments.
    fn blob_store(&self) -> Arc<dyn mailquill_core::blob::BlobStore>;
    /// Key that decrypts the account credentials stored in the user database.
    fn credential_key(&self) -> Arc<mailquill_core::crypto::CredentialKey>;
    /// The registry the sync tasks report their status to.
    fn sync_manager(&self) -> Arc<SyncManager>;
    /// Deliver a new-message notification to the user. The default does
    /// nothing; the api crate broadcasts it over SSE and web push.
    async fn notify_new_message(
        &self,
        _user_id: &str,
        _message: NewMessageNotification,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Publish a sync status change to the user. The default does nothing;
    /// the api crate broadcasts it over SSE.
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
    async fn deliver_returns_at_once_and_the_command_still_arrives() {
        let (tx, mut rx) = mpsc::channel(1);
        tx.try_send(SyncCommand::ForcePoll).unwrap();

        // Channel is full: the HTTP handler calling this must not wait.
        tokio::time::timeout(Duration::from_millis(50), async {
            SyncManager::deliver(
                tx.clone(),
                SyncCommand::FlushMessageOps {
                    user_id: "user".into(),
                },
            );
        })
        .await
        .expect("deliver must not block on a full channel");

        assert!(matches!(rx.recv().await, Some(SyncCommand::ForcePoll)));
        let delivered = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("the background send completes once the task drains");
        assert!(matches!(delivered, Some(SyncCommand::FlushMessageOps { .. })));
    }

    #[tokio::test]
    async fn deliver_to_a_stopped_task_is_a_no_op() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        SyncManager::deliver(tx, SyncCommand::ForcePoll);
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
                    .queue_message_op_flush("user".into(), "busy")
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
                .queue_message_op_flush("user".into(), "other")
                .await;
        })
        .await
        .expect("the registry lock must not be held while waiting on a full channel");
        assert!(matches!(
            rx.try_recv(),
            Ok(SyncCommand::FlushMessageOps { .. })
        ));
        blocked.abort();
    }
}
