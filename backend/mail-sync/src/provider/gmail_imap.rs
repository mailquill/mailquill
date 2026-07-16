//! Gmail-over-IMAP hybrid provider.
//!
//! Mail sync (list/fetch/flags/status/idle) runs over IMAP+XOAUTH2 — the same
//! path as any IMAP account — so it reuses [`ImapProvider`]. Only **label
//! handling** (move/archive/delete) goes through the Gmail API, because Gmail
//! labels (multi-label messages, "archive" = drop the INBOX label) don't map
//! cleanly onto IMAP MOVE.
//!
//! Bridge: during fetch we pull the Gmail `X-GM-MSGID` extension and store its
//! hex form (the Gmail API message id) in [`IdMap`] keyed by the IMAP `(folder,
//! uid)`. Label ops resolve that id and call the Gmail REST API. The folder a
//! move targets is classified by its locale-independent `folder_type` (from the
//! IMAP SPECIAL-USE flags), not by name.

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::json;
use sqlx::SqlitePool;

use super::http::Rest;
use super::idmap::IdMap;
use super::imap::ImapProvider;
use super::{FolderStatus, MailProvider, ProviderConfig, ProviderError};
use crate::session::{self, FetchedMessage, FolderInfo};

const BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";

pub struct GmailImapProvider {
    imap: ImapProvider,
    rest: Rest,
    ids: IdMap,
    db: SqlitePool,
    account_id: String,
    /// Lazily loaded user-label name → label id (system labels are constant).
    label_ids: Option<HashMap<String, String>>,
}

impl GmailImapProvider {
    pub async fn connect(config: &ProviderConfig) -> Result<Self, ProviderError> {
        let token = config.oauth_access_token.clone().ok_or_else(|| {
            ProviderError::Other("gmail_imap requires an OAuth access token".into())
        })?;
        let db = config
            .db
            .clone()
            .ok_or_else(|| ProviderError::Other("gmail_imap requires a user db handle".into()))?;
        let imap = ImapProvider::connect(config).await?;
        Ok(Self {
            imap,
            rest: Rest::new(token),
            ids: IdMap::new(db.clone(), config.account_id.clone()),
            db,
            account_id: config.account_id.clone(),
            label_ids: None,
        })
    }

    /// Bind each fetched IMAP uid to its Gmail message id (hex X-GM-MSGID) so a
    /// later label op can address it via the API. Best-effort: a failure here
    /// must not break the fetch itself.
    async fn record_gmail_ids(&mut self, folder: &str, uid_set: &str) {
        match session::fetch_gmail_msgids(self.imap.session_mut(), uid_set).await {
            Ok(pairs) => {
                for (uid, remote) in pairs {
                    let _ = self.ids.set(folder, uid, &remote).await;
                }
            }
            Err(e) => tracing::warn!(
                "gmail X-GM-MSGID fetch failed: account={} folder={folder} uid_set={uid_set} err={e}",
                self.account_id
            ),
        }
    }

    /// `folder_type` of a stored folder (INBOX/ARCHIVE/TRASH/…), used to decide
    /// the Gmail label operation independent of the (localized) folder name.
    async fn folder_type(&self, full_path: &str) -> String {
        sqlx::query_scalar::<_, String>(
            "SELECT folder_type FROM folders WHERE account_id = ? AND full_path = ?",
        )
        .bind(&self.account_id)
        .bind(full_path)
        .fetch_optional(&self.db)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "CUSTOM".to_string())
    }

    /// Gmail label id for a folder. System types map to fixed ids; custom labels
    /// resolve by name through the Labels API.
    async fn label_id(&mut self, full_path: &str, folder_type: &str) -> Option<String> {
        match folder_type {
            "INBOX" => Some("INBOX".into()),
            "SENT" => Some("SENT".into()),
            "DRAFTS" => Some("DRAFT".into()),
            "SPAM" => Some("SPAM".into()),
            "TRASH" => Some("TRASH".into()),
            // \All Mail: no addable label (archive = absence of INBOX).
            "ARCHIVE" => None,
            _ => self.user_label_id(full_path).await,
        }
    }

    async fn user_label_id(&mut self, name: &str) -> Option<String> {
        if self.label_ids.is_none() {
            let mut map = HashMap::new();
            if let Ok(res) = self.rest.get_json(&format!("{BASE}/labels")).await {
                for label in res["labels"].as_array().unwrap_or(&Vec::new()) {
                    if let (Some(id), Some(n)) = (label["id"].as_str(), label["name"].as_str()) {
                        map.insert(n.to_owned(), id.to_owned());
                    }
                }
            }
            self.label_ids = Some(map);
        }
        self.label_ids.as_ref().and_then(|m| m.get(name).cloned())
    }

    async fn modify_labels(
        &self,
        remote_id: &str,
        add: &[&str],
        remove: &[&str],
    ) -> Result<(), ProviderError> {
        self.rest
            .post_json(
                &format!("{BASE}/messages/{remote_id}/modify"),
                &json!({ "addLabelIds": add, "removeLabelIds": remove }),
            )
            .await?;
        Ok(())
    }
}

#[async_trait]
impl MailProvider for GmailImapProvider {
    async fn list_folders(&mut self) -> Result<Vec<FolderInfo>, ProviderError> {
        self.imap.list_folders().await
    }

    async fn folder_status(&mut self, folder: &str) -> Result<FolderStatus, ProviderError> {
        self.imap.folder_status(folder).await
    }

    async fn highest_uid(&mut self, folder: &str) -> Result<Option<u32>, ProviderError> {
        self.imap.highest_uid(folder).await
    }

    async fn fetch_flags(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<(u32, bool, bool, bool)>, ProviderError> {
        self.imap.fetch_flags(folder, uid_set).await
    }

    async fn fetch_headers(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<FetchedMessage>, ProviderError> {
        let msgs = self.imap.fetch_headers(folder, uid_set).await?;
        self.record_gmail_ids(folder, uid_set).await;
        Ok(msgs)
    }

    async fn fetch_full(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<FetchedMessage>, ProviderError> {
        let msgs = self.imap.fetch_full(folder, uid_set).await?;
        self.record_gmail_ids(folder, uid_set).await;
        Ok(msgs)
    }

    async fn fetch_raw(&mut self, folder: &str, uid: u32) -> Result<Vec<u8>, ProviderError> {
        self.imap.fetch_raw(folder, uid).await
    }

    async fn set_flag(
        &mut self,
        folder: &str,
        uid: u32,
        flag: &str,
        value: bool,
    ) -> Result<(), ProviderError> {
        self.imap.set_flag(folder, uid, flag, value).await
    }

    /// Move = relabel via the Gmail API. Trash → trash; All Mail (\All) →
    /// archive (drop INBOX); otherwise add the destination label and drop the
    /// source label.
    async fn move_message(
        &mut self,
        src_folder: &str,
        uid: u32,
        dest_folder: &str,
    ) -> Result<(), ProviderError> {
        let remote = self.ids.remote_id(src_folder, uid).await?;
        let dest_type = self.folder_type(dest_folder).await;

        match dest_type.as_str() {
            "TRASH" => {
                self.rest
                    .post_json(&format!("{BASE}/messages/{remote}/trash"), &json!({}))
                    .await?;
            }
            "ARCHIVE" => {
                self.modify_labels(&remote, &[], &["INBOX"]).await?;
            }
            _ => {
                let src_type = self.folder_type(src_folder).await;
                let add = self.label_id(dest_folder, &dest_type).await;
                let remove = self.label_id(src_folder, &src_type).await;
                let add_ids: Vec<&str> = add.as_deref().into_iter().collect();
                let remove_ids: Vec<&str> = remove.as_deref().into_iter().collect();
                self.modify_labels(&remote, &add_ids, &remove_ids).await?;
            }
        }
        // Re-discovered with a fresh uid wherever it now lives.
        self.ids.remove(src_folder, uid).await
    }

    async fn delete_permanently(&mut self, folder: &str, uid: u32) -> Result<(), ProviderError> {
        let remote = self.ids.remote_id(folder, uid).await?;
        self.rest
            .delete(&format!("{BASE}/messages/{remote}"))
            .await?;
        self.ids.remove(folder, uid).await
    }

    async fn append_sent(&mut self, raw_message: &[u8]) -> Result<(), ProviderError> {
        self.imap.append_sent(raw_message).await
    }

    async fn close(&mut self) -> Result<(), ProviderError> {
        self.imap.close().await
    }
}
