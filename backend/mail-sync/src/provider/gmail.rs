//! Gmail API implementation of [`MailProvider`].
//!
//! Talks to `gmail.googleapis.com/gmail/v1/users/me` with the account's OAuth
//! access token (scope `https://mail.google.com/` for delete; `gmail.modify`
//! covers everything else). Labels act as folders: a folder's `full_path` is
//! the **label id** (`INBOX`, `SENT`, user label ids like `Label_12`).
//!
//! Gmail message ids are opaque strings; the pipeline addresses messages by
//! per-folder integer uid. [`IdMap`] (table `remote_message_ids`) bridges the
//! two: `highest_uid` discovers new message ids (newest-first listing, stopped
//! at the first already-known id) and assigns ascending uids oldest-first, so
//! the pipeline's `last_uid` incremental logic works unchanged.

use async_trait::async_trait;
use base64::Engine;
use serde_json::{json, Value};

use super::http::Rest;
use super::idmap::IdMap;
use super::util::{fetched_from_raw, header_block_from_pairs, rfc3339_from_millis};
use super::{FolderStatus, MailProvider, ProviderConfig, ProviderError};
use crate::session::{FetchedMessage, FolderInfo};

const BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
/// Pagination page size and safety cap for one discovery pass. A pass picks up
/// where the previous one stopped (everything new stays undiscovered until the
/// next sync tick), so the cap bounds work per tick without losing mail.
const PAGE_SIZE: u32 = 500;
const MAX_DISCOVERY_PAGES: u32 = 20;

pub struct GmailProvider {
    rest: Rest,
    ids: IdMap,
}

impl GmailProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self, ProviderError> {
        let token = config
            .oauth_access_token
            .clone()
            .ok_or(ProviderError::Other("gmail api requires an OAuth access token".into()))?;
        let db = config
            .db
            .clone()
            .ok_or(ProviderError::Other("gmail api requires a user db handle".into()))?;
        Ok(Self {
            rest: Rest::new(token),
            ids: IdMap::new(db, config.account_id.clone()),
        })
    }

    /// Newest-first message ids in a label until an already-known id (or the
    /// page cap) is hit; returns them oldest-first for uid assignment.
    async fn discover_new(&self, label_id: &str) -> Result<Vec<String>, ProviderError> {
        let known = self.ids.known_ids(label_id).await?;
        let mut new_ids: Vec<String> = Vec::new();
        let mut page_token: Option<String> = None;

        'pages: for _ in 0..MAX_DISCOVERY_PAGES {
            let mut url = format!(
                "{BASE}/messages?labelIds={}&maxResults={PAGE_SIZE}",
                urlencoding::encode(label_id)
            );
            if let Some(ref t) = page_token {
                url.push_str(&format!("&pageToken={t}"));
            }
            let res = self.rest.get_json(&url).await?;

            for m in res["messages"].as_array().unwrap_or(&Vec::new()) {
                let Some(id) = m["id"].as_str() else { continue };
                if known.contains(id) {
                    break 'pages;
                }
                new_ids.push(id.to_owned());
            }

            match res["nextPageToken"].as_str() {
                Some(t) => page_token = Some(t.to_owned()),
                None => break,
            }
        }

        new_ids.reverse(); // oldest first
        Ok(new_ids)
    }

    fn flags_from_labels(labels: &Value) -> (bool, bool) {
        let labels = labels.as_array().map(Vec::as_slice).unwrap_or(&[]);
        let has = |l: &str| labels.iter().any(|v| v.as_str() == Some(l));
        (!has("UNREAD"), has("STARRED"))
    }

    async fn fetch_metadata(&self, uid: u32, remote_id: &str) -> Result<FetchedMessage, ProviderError> {
        let res = self
            .rest
            .get_json(&format!("{BASE}/messages/{remote_id}?format=metadata"))
            .await?;
        let (is_seen, is_flagged) = Self::flags_from_labels(&res["labelIds"]);
        let internal_date = rfc3339_from_millis(
            res["internalDate"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0),
        );

        let empty = Vec::new();
        let headers = res["payload"]["headers"].as_array().unwrap_or(&empty);
        let block = header_block_from_pairs(headers.iter().filter_map(|h| {
            Some((h["name"].as_str()?, h["value"].as_str()?))
        }));

        Ok(fetched_from_raw(uid, &block, internal_date, is_seen, is_flagged, false))
    }

    async fn fetch_raw_message(&self, remote_id: &str) -> Result<(Vec<u8>, Value), ProviderError> {
        let res = self
            .rest
            .get_json(&format!("{BASE}/messages/{remote_id}?format=raw"))
            .await?;
        let raw_b64 = res["raw"].as_str().unwrap_or_default();
        let raw = base64::engine::general_purpose::URL_SAFE
            .decode(raw_b64)
            .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(raw_b64))
            .map_err(|e| ProviderError::Other(format!("gmail raw decode: {e}")))?;
        Ok((raw, res))
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
impl MailProvider for GmailProvider {
    async fn list_folders(&mut self) -> Result<Vec<FolderInfo>, ProviderError> {
        let res = self.rest.get_json(&format!("{BASE}/labels")).await?;
        let mut folders = Vec::new();
        for label in res["labels"].as_array().unwrap_or(&Vec::new()) {
            let id = label["id"].as_str().unwrap_or_default().to_owned();
            let name = label["name"].as_str().unwrap_or_default().to_owned();
            let label_type = label["type"].as_str().unwrap_or("user");

            let folder_type = match id.as_str() {
                "INBOX" => "INBOX",
                "SENT" => "SENT",
                "DRAFT" => "DRAFTS",
                "SPAM" => "SPAM",
                "TRASH" => "TRASH",
                // Bookkeeping labels, not mailboxes.
                "UNREAD" | "STARRED" | "IMPORTANT" | "CHAT" => continue,
                _ if id.starts_with("CATEGORY_") => continue,
                _ if label_type == "user" => "CUSTOM",
                _ => continue,
            };

            folders.push(FolderInfo {
                name,
                full_path: id,
                folder_type: folder_type.into(),
            });
        }
        Ok(folders)
    }

    async fn folder_status(&mut self, folder: &str) -> Result<FolderStatus, ProviderError> {
        let res = self
            .rest
            .get_json(&format!("{BASE}/labels/{}", urlencoding::encode(folder)))
            .await?;
        Ok(FolderStatus {
            // The uid mapping is locally owned — no server-side generation marker.
            uidvalidity: 1,
            exists: res["messagesTotal"].as_u64().unwrap_or(0) as u32,
        })
    }

    async fn highest_uid(&mut self, folder: &str) -> Result<Option<u32>, ProviderError> {
        let new_ids = self.discover_new(folder).await?;
        let max = if new_ids.is_empty() {
            self.ids.max_uid(folder).await?
        } else {
            self.ids.assign(folder, &new_ids).await?
        };
        Ok((max > 0).then_some(max))
    }

    async fn fetch_flags(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<(u32, bool, bool, bool)>, ProviderError> {
        let mut out = Vec::new();
        for (uid, remote_id) in self.ids.resolve_set(folder, uid_set).await? {
            let res = self
                .rest
                .get_json(&format!("{BASE}/messages/{remote_id}?format=minimal"))
                .await;
            match res {
                Ok(v) => {
                    let (seen, flagged) = Self::flags_from_labels(&v["labelIds"]);
                    // API providers move atomically; no IMAP-style \Deleted ghost.
                    out.push((uid, seen, flagged, false));
                }
                // Message gone (deleted/moved on the server) — drop the mapping.
                Err(_) => self.ids.remove(folder, uid).await?,
            }
        }
        Ok(out)
    }

    async fn fetch_headers(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<FetchedMessage>, ProviderError> {
        let mut out = Vec::new();
        for (uid, remote_id) in self.ids.resolve_set(folder, uid_set).await? {
            match self.fetch_metadata(uid, &remote_id).await {
                Ok(m) => out.push(m),
                Err(e) => tracing::warn!("gmail: metadata fetch failed for {remote_id}: {e}"),
            }
        }
        Ok(out)
    }

    async fn fetch_full(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<FetchedMessage>, ProviderError> {
        let mut out = Vec::new();
        for (uid, remote_id) in self.ids.resolve_set(folder, uid_set).await? {
            match self.fetch_raw_message(&remote_id).await {
                Ok((raw, meta)) => {
                    let (is_seen, is_flagged) = Self::flags_from_labels(&meta["labelIds"]);
                    let internal_date = rfc3339_from_millis(
                        meta["internalDate"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0),
                    );
                    out.push(fetched_from_raw(uid, &raw, internal_date, is_seen, is_flagged, true));
                }
                Err(e) => tracing::warn!("gmail: raw fetch failed for {remote_id}: {e}"),
            }
        }
        Ok(out)
    }

    async fn fetch_raw(&mut self, folder: &str, uid: u32) -> Result<Vec<u8>, ProviderError> {
        let remote_id = self.ids.remote_id(folder, uid).await?;
        Ok(self.fetch_raw_message(&remote_id).await?.0)
    }

    async fn set_flag(
        &mut self,
        folder: &str,
        uid: u32,
        flag: &str,
        value: bool,
    ) -> Result<(), ProviderError> {
        let remote_id = self.ids.remote_id(folder, uid).await?;
        match (flag, value) {
            ("seen", true) => self.modify_labels(&remote_id, &[], &["UNREAD"]).await,
            ("seen", false) => self.modify_labels(&remote_id, &["UNREAD"], &[]).await,
            ("flagged", true) => self.modify_labels(&remote_id, &["STARRED"], &[]).await,
            ("flagged", false) => self.modify_labels(&remote_id, &[], &["STARRED"]).await,
            ("deleted", true) => {
                self.rest
                    .post_json(&format!("{BASE}/messages/{remote_id}/trash"), &json!({}))
                    .await?;
                self.ids.remove(folder, uid).await
            }
            ("deleted", false) => {
                self.rest
                    .post_json(&format!("{BASE}/messages/{remote_id}/untrash"), &json!({}))
                    .await?;
                Ok(())
            }
            (other, _) => Err(ProviderError::Other(format!("unknown flag: {other}"))),
        }
    }

    async fn move_message(
        &mut self,
        src_folder: &str,
        uid: u32,
        dest_folder: &str,
    ) -> Result<(), ProviderError> {
        let remote_id = self.ids.remote_id(src_folder, uid).await?;
        if dest_folder == "TRASH" {
            self.rest
                .post_json(&format!("{BASE}/messages/{remote_id}/trash"), &json!({}))
                .await?;
        } else {
            self.modify_labels(&remote_id, &[dest_folder], &[src_folder]).await?;
        }
        // Re-discovered with a fresh uid in the destination label.
        self.ids.remove(src_folder, uid).await
    }

    async fn delete_permanently(&mut self, folder: &str, uid: u32) -> Result<(), ProviderError> {
        let remote_id = self.ids.remote_id(folder, uid).await?;
        self.rest.delete(&format!("{BASE}/messages/{remote_id}")).await?;
        self.ids.remove(folder, uid).await
    }

    async fn append_sent(&mut self, _raw_message: &[u8]) -> Result<(), ProviderError> {
        // Gmail stores sent mail itself when sending via the API.
        Ok(())
    }

    async fn send_message(&mut self, raw_message: &[u8]) -> Result<(), ProviderError> {
        let raw = base64::engine::general_purpose::URL_SAFE.encode(raw_message);
        self.rest
            .post_json(&format!("{BASE}/messages/send"), &json!({ "raw": raw }))
            .await?;
        Ok(())
    }

    async fn close(&mut self) -> Result<(), ProviderError> {
        Ok(())
    }
}
