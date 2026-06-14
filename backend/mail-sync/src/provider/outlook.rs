//! Microsoft Graph (Outlook) implementation of [`MailProvider`].
//!
//! Talks to `graph.microsoft.com/v1.0/me` with the account's OAuth access
//! token (scope `Mail.ReadWrite`). A folder's `full_path` is the Graph
//! **mailFolder id**; well-known folders map to normalized folder types.
//!
//! Graph message ids are opaque strings (and change when a message is moved!),
//! so the same [`IdMap`] bridging as Gmail applies: discovery assigns
//! ascending per-folder uids, moves drop the source mapping and the message is
//! re-discovered at its destination.

use async_trait::async_trait;
use serde_json::{json, Value};

use super::http::Rest;
use super::idmap::IdMap;
use super::util::{fetched_from_raw, header_block_from_pairs};
use super::{FolderStatus, MailProvider, ProviderConfig, ProviderError};
use crate::session::{FetchedMessage, FolderInfo};

const BASE: &str = "https://graph.microsoft.com/v1.0/me";
const PAGE_SIZE: u32 = 100;
const MAX_DISCOVERY_PAGES: u32 = 50;

pub struct OutlookProvider {
    rest: Rest,
    ids: IdMap,
}

impl OutlookProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self, ProviderError> {
        let token = config
            .oauth_access_token
            .clone()
            .ok_or(ProviderError::Other("outlook api requires an OAuth access token".into()))?;
        let db = config
            .db
            .clone()
            .ok_or(ProviderError::Other("outlook api requires a user db handle".into()))?;
        Ok(Self {
            rest: Rest::new(token),
            ids: IdMap::new(db, config.account_id.clone()),
        })
    }

    /// Newest-first ids in a folder until an already-known id (or the page
    /// cap) is hit; returned oldest-first for uid assignment.
    async fn discover_new(&self, folder_id: &str) -> Result<Vec<String>, ProviderError> {
        let known = self.ids.known_ids(folder_id).await?;
        let mut new_ids: Vec<String> = Vec::new();
        let mut url = format!(
            "{BASE}/mailFolders/{folder_id}/messages?$select=id&$orderby=receivedDateTime desc&$top={PAGE_SIZE}"
        );

        'pages: for _ in 0..MAX_DISCOVERY_PAGES {
            let res = self.rest.get_json(&url).await?;
            for m in res["value"].as_array().unwrap_or(&Vec::new()) {
                let Some(id) = m["id"].as_str() else { continue };
                if known.contains(id) {
                    break 'pages;
                }
                new_ids.push(id.to_owned());
            }
            match res["@odata.nextLink"].as_str() {
                Some(next) => url = next.to_owned(),
                None => break,
            }
        }

        new_ids.reverse();
        Ok(new_ids)
    }

    async fn fetch_metadata(&self, uid: u32, remote_id: &str) -> Result<FetchedMessage, ProviderError> {
        let res = self
            .rest
            .get_json(&format!(
                "{BASE}/messages/{remote_id}?$select=internetMessageHeaders,receivedDateTime,isRead,flag"
            ))
            .await?;

        let (is_seen, is_flagged, internal_date) = Self::state_of(&res);
        let empty = Vec::new();
        let headers = res["internetMessageHeaders"].as_array().unwrap_or(&empty);
        let block = header_block_from_pairs(headers.iter().filter_map(|h| {
            Some((h["name"].as_str()?, h["value"].as_str()?))
        }));

        Ok(fetched_from_raw(uid, &block, internal_date, is_seen, is_flagged, false))
    }

    fn state_of(message: &Value) -> (bool, bool, String) {
        let is_seen = message["isRead"].as_bool().unwrap_or(false);
        let is_flagged = message["flag"]["flagStatus"].as_str() == Some("flagged");
        let internal_date = message["receivedDateTime"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        (is_seen, is_flagged, internal_date)
    }
}

#[async_trait]
impl MailProvider for OutlookProvider {
    async fn list_folders(&mut self) -> Result<Vec<FolderInfo>, ProviderError> {
        let res = self
            .rest
            .get_json(&format!("{BASE}/mailFolders?$top=200&$select=id,displayName,wellKnownName"))
            .await?;

        let mut folders = Vec::new();
        for f in res["value"].as_array().unwrap_or(&Vec::new()) {
            let id = f["id"].as_str().unwrap_or_default().to_owned();
            let name = f["displayName"].as_str().unwrap_or_default().to_owned();
            let folder_type = match f["wellKnownName"].as_str().unwrap_or("") {
                "inbox" => "INBOX",
                "sentitems" => "SENT",
                "drafts" => "DRAFTS",
                "archive" => "ARCHIVE",
                "junkemail" => "SPAM",
                "deleteditems" => "TRASH",
                // Internal folders that are not user mailboxes.
                "outbox" | "conversationhistory" | "recoverableitemsdeletions" | "scheduled"
                | "searchfolders" | "serverfailures" | "syncissues" | "conflicts"
                | "localfailures" | "clutter" => continue,
                _ => "CUSTOM",
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
            .get_json(&format!("{BASE}/mailFolders/{folder}?$select=totalItemCount"))
            .await?;
        Ok(FolderStatus {
            uidvalidity: 1,
            exists: res["totalItemCount"].as_u64().unwrap_or(0) as u32,
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
                .get_json(&format!("{BASE}/messages/{remote_id}?$select=isRead,flag"))
                .await;
            match res {
                Ok(v) => {
                    let (seen, flagged, _) = Self::state_of(&v);
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
                Err(e) => tracing::warn!("outlook: metadata fetch failed for {remote_id}: {e}"),
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
            let meta = self
                .rest
                .get_json(&format!("{BASE}/messages/{remote_id}?$select=isRead,flag,receivedDateTime"))
                .await;
            let raw = self.rest.get_bytes(&format!("{BASE}/messages/{remote_id}/$value")).await;
            match (meta, raw) {
                (Ok(meta), Ok(raw)) => {
                    let (is_seen, is_flagged, internal_date) = Self::state_of(&meta);
                    out.push(fetched_from_raw(uid, &raw, internal_date, is_seen, is_flagged, true));
                }
                (Err(e), _) | (_, Err(e)) => {
                    tracing::warn!("outlook: full fetch failed for {remote_id}: {e}");
                }
            }
        }
        Ok(out)
    }

    async fn fetch_raw(&mut self, folder: &str, uid: u32) -> Result<Vec<u8>, ProviderError> {
        let remote_id = self.ids.remote_id(folder, uid).await?;
        self.rest.get_bytes(&format!("{BASE}/messages/{remote_id}/$value")).await
    }

    async fn set_flag(
        &mut self,
        folder: &str,
        uid: u32,
        flag: &str,
        value: bool,
    ) -> Result<(), ProviderError> {
        let remote_id = self.ids.remote_id(folder, uid).await?;
        let url = format!("{BASE}/messages/{remote_id}");
        match flag {
            "seen" => self.rest.patch_json(&url, &json!({ "isRead": value })).await,
            "flagged" => {
                let status = if value { "flagged" } else { "notFlagged" };
                self.rest.patch_json(&url, &json!({ "flag": { "flagStatus": status } })).await
            }
            "deleted" => {
                if value {
                    self.rest
                        .post_json(
                            &format!("{BASE}/messages/{remote_id}/move"),
                            &json!({ "destinationId": "deleteditems" }),
                        )
                        .await?;
                    self.ids.remove(folder, uid).await
                } else {
                    Ok(())
                }
            }
            other => Err(ProviderError::Other(format!("unknown flag: {other}"))),
        }
    }

    async fn move_message(
        &mut self,
        src_folder: &str,
        uid: u32,
        dest_folder: &str,
    ) -> Result<(), ProviderError> {
        let remote_id = self.ids.remote_id(src_folder, uid).await?;
        // Graph returns the message's NEW id after a move; we don't track it —
        // the destination folder re-discovers the message on its next sync.
        self.rest
            .post_json(
                &format!("{BASE}/messages/{remote_id}/move"),
                &json!({ "destinationId": dest_folder }),
            )
            .await?;
        self.ids.remove(src_folder, uid).await
    }

    async fn delete_permanently(&mut self, folder: &str, uid: u32) -> Result<(), ProviderError> {
        let remote_id = self.ids.remote_id(folder, uid).await?;
        self.rest.delete(&format!("{BASE}/messages/{remote_id}")).await?;
        self.ids.remove(folder, uid).await
    }

    async fn append_sent(&mut self, _raw_message: &[u8]) -> Result<(), ProviderError> {
        // Graph stores sent mail itself when sending via the API.
        Ok(())
    }

    async fn send_message(&mut self, raw_message: &[u8]) -> Result<(), ProviderError> {
        use base64::Engine;
        let body = base64::engine::general_purpose::STANDARD.encode(raw_message);
        self.rest
            .post_raw(&format!("{BASE}/sendMail"), body, "text/plain")
            .await
    }

    async fn close(&mut self) -> Result<(), ProviderError> {
        Ok(())
    }
}
