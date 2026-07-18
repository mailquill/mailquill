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
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
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

    async fn fetch_gmail_ids(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<(u32, String)>, ProviderError> {
        self.imap.ensure_selected(folder).await?;
        session::fetch_gmail_msgids(self.imap.session_mut(), uid_set)
            .await
            .map_err(ProviderError::from)
    }

    /// Attach each Gmail id to the fetched message. The sync pipeline persists
    /// it in the same transaction as the message metadata, avoiding a second
    /// SQLite writer acquisition for every IMAP fetch.
    async fn attach_gmail_ids(
        &mut self,
        folder: &str,
        uid_set: &str,
        messages: &mut [FetchedMessage],
    ) -> Result<(), ProviderError> {
        let pairs: HashMap<u32, String> = self
            .fetch_gmail_ids(folder, uid_set)
            .await?
            .into_iter()
            .collect();
        for message in messages {
            message.remote_id = pairs.get(&message.uid).cloned();
        }
        Ok(())
    }

    /// Resolve the Gmail API id lazily if the best-effort correlation during
    /// import was interrupted. This keeps label actions repairable without
    /// downloading the message again.
    async fn gmail_message_id(&mut self, folder: &str, uid: u32) -> Result<String, ProviderError> {
        if let Ok(remote_id) = self.ids.remote_id(folder, uid).await {
            return Ok(remote_id);
        }
        let pairs = self.fetch_gmail_ids(folder, &uid.to_string()).await?;
        self.ids.set_many(folder, &pairs).await?;
        self.ids.remote_id(folder, uid).await.map_err(|_| {
            ProviderError::Other(format!(
                "Gmail did not return X-GM-MSGID for folder {folder} uid {uid}"
            ))
        })
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
    async fn label_id(
        &mut self,
        full_path: &str,
        folder_type: &str,
    ) -> Result<Option<String>, ProviderError> {
        match folder_type {
            "INBOX" => Ok(Some("INBOX".into())),
            "SENT" => Ok(Some("SENT".into())),
            "DRAFTS" => Ok(Some("DRAFT".into())),
            "SPAM" => Ok(Some("SPAM".into())),
            "TRASH" => Ok(Some("TRASH".into())),
            // \All Mail: no addable label (archive = absence of INBOX).
            "ARCHIVE" => Ok(None),
            _ => self.user_label_id(full_path).await.map(Some),
        }
    }

    async fn user_label_id(&mut self, imap_path: &str) -> Result<String, ProviderError> {
        if self.label_ids.is_none() {
            let mut map = HashMap::new();
            let res = self
                .rest
                .get_json(&format!("{BASE}/labels?fields=labels(id,name,type)"))
                .await?;
            for label in res["labels"].as_array().unwrap_or(&Vec::new()) {
                if let (Some(id), Some(name)) = (label["id"].as_str(), label["name"].as_str()) {
                    map.insert(name.to_owned(), id.to_owned());
                }
            }
            self.label_ids = Some(map);
        }
        let decoded_path = decode_modified_utf7(imap_path);
        self.label_ids
            .as_ref()
            .and_then(|labels| {
                labels
                    .get(imap_path)
                    .or_else(|| labels.get(&decoded_path))
                    .cloned()
            })
            .ok_or_else(|| {
                ProviderError::Other(format!(
                    "Gmail label API did not return a label matching IMAP folder {imap_path}"
                ))
            })
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
        let mut msgs = self.imap.fetch_headers(folder, uid_set).await?;
        if let Err(error) = self.attach_gmail_ids(folder, uid_set, &mut msgs).await {
            tracing::warn!(
                "gmail X-GM-MSGID correlation failed: account={} folder={folder} uid_set={uid_set} error={error}",
                self.account_id
            );
        }
        Ok(msgs)
    }

    async fn fetch_full(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<FetchedMessage>, ProviderError> {
        let mut msgs = self.imap.fetch_full(folder, uid_set).await?;
        if let Err(error) = self.attach_gmail_ids(folder, uid_set, &mut msgs).await {
            tracing::warn!(
                "gmail X-GM-MSGID correlation failed: account={} folder={folder} uid_set={uid_set} error={error}",
                self.account_id
            );
        }
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
        let remote = self.gmail_message_id(src_folder, uid).await?;
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
                let add = self.label_id(dest_folder, &dest_type).await?;
                let remove = self.label_id(src_folder, &src_type).await?;
                let add_ids: Vec<&str> = add.as_deref().into_iter().collect();
                let remove_ids: Vec<&str> = remove.as_deref().into_iter().collect();
                self.modify_labels(&remote, &add_ids, &remove_ids).await?;
            }
        }
        // Re-discovered with a fresh uid wherever it now lives.
        self.ids.remove(src_folder, uid).await
    }

    async fn delete_permanently(&mut self, folder: &str, uid: u32) -> Result<(), ProviderError> {
        let remote = self.gmail_message_id(folder, uid).await?;
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

fn decode_modified_utf7(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'&' {
            let character = input[index..].chars().next().expect("valid UTF-8");
            output.push(character);
            index += character.len_utf8();
            continue;
        }

        let Some(relative_end) = bytes[index + 1..].iter().position(|byte| *byte == b'-') else {
            output.push_str(&input[index..]);
            break;
        };
        let end = index + 1 + relative_end;
        let encoded = &input[index + 1..end];
        if encoded.is_empty() {
            output.push('&');
        } else if let Some(decoded) = decode_modified_utf7_run(encoded) {
            output.push_str(&decoded);
        } else {
            output.push_str(&input[index..=end]);
        }
        index = end + 1;
    }
    output
}

fn decode_modified_utf7_run(encoded: &str) -> Option<String> {
    let standard = encoded.replace(',', "/");
    let bytes = STANDARD_NO_PAD.decode(standard).ok()?;
    if bytes.len() % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
        .collect();
    String::from_utf16(&units).ok()
}

#[cfg(test)]
mod tests {
    use super::decode_modified_utf7;

    #[test]
    fn decodes_custom_label_names_for_api_correlation() {
        assert_eq!(decode_modified_utf7("J&APw-licher"), "Jülicher");
        assert_eq!(decode_modified_utf7("R&D-&-"), "R&D-&");
    }
}
