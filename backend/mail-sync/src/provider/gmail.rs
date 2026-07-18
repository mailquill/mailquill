//! Gmail API implementation of [`MailProvider`].
//!
//! Talks to `gmail.googleapis.com/gmail/v1/users/me` with the account's OAuth
//! access token (scope `https://mail.google.com/` for delete; `gmail.modify`
//! covers everything else). Labels act as folders: a folder's `full_path` is
//! the **label id** (`INBOX`, `SENT`, user label ids like `Label_12`).
//!
//! Gmail message ids are opaque strings; the pipeline addresses messages by
//! per-folder integer uid. [`IdMap`] (table `remote_message_ids`) bridges the
//! two: `highest_uid` discovers new message ids and assigns ascending uids
//! oldest-first, so the pipeline's `last_uid` incremental logic works
//! unchanged. Initial discovery persists Gmail's opaque next-page token so a
//! bounded pass can resume through labels larger than the page limit.

use async_trait::async_trait;
use base64::Engine;
use futures::{stream, StreamExt};
use serde_json::{json, Value};
use std::collections::HashSet;

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
const CONCURRENT_FETCHES: usize = 8;

pub struct GmailProvider {
    rest: Rest,
    ids: IdMap,
    account_id: String,
}

impl GmailProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self, ProviderError> {
        let token = config
            .oauth_access_token
            .clone()
            .ok_or(ProviderError::Other(
                "gmail api requires an OAuth access token".into(),
            ))?;
        let db = config.db.clone().ok_or(ProviderError::Other(
            "gmail api requires a user db handle".into(),
        ))?;
        Ok(Self {
            rest: Rest::new(token),
            ids: IdMap::new(db, config.account_id.clone()),
            account_id: config.account_id.clone(),
        })
    }

    fn encode_raw_message(raw_message: &[u8]) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw_message)
    }

    /// Discover one bounded batch and return its next durable backfill state.
    async fn discover_new(
        &self,
        label_id: &str,
    ) -> Result<(Vec<String>, Option<String>, bool), ProviderError> {
        let known = self.ids.known_ids(label_id).await?;
        let backfill = self.ids.backfill_state(label_id).await?;
        let mut new_ids: Vec<String> = Vec::new();
        let mut page_token = if backfill.complete {
            None
        } else {
            backfill.page_token
        };
        let mut can_reset_stale_cursor = !backfill.complete && page_token.is_some();
        let mut complete = backfill.complete;
        let mut stopped_at_known = false;

        'pages: for _ in 0..MAX_DISCOVERY_PAGES {
            let mut url = format!(
                "{BASE}/messages?labelIds={}&maxResults={PAGE_SIZE}&fields=messages/id,nextPageToken",
                urlencoding::encode(label_id)
            );
            if let Some(ref t) = page_token {
                url.push_str(&format!("&pageToken={}", urlencoding::encode(t)));
            }
            let res = match self.rest.get_json(&url).await {
                Ok(response) => {
                    can_reset_stale_cursor = false;
                    response
                }
                // Gmail page tokens are opaque and may expire. Reset a stale
                // persisted cursor once instead of failing every future sync.
                Err(ProviderError::Http { status: 400, .. }) if can_reset_stale_cursor => {
                    self.ids.set_backfill_state(label_id, None, false).await?;
                    page_token = None;
                    can_reset_stale_cursor = false;
                    continue 'pages;
                }
                Err(error) => return Err(error),
            };

            if Self::collect_page_ids(&res["messages"], &known, complete, &mut new_ids) {
                stopped_at_known = true;
                page_token = None;
                break 'pages;
            }

            match res["nextPageToken"].as_str() {
                Some(t) => page_token = Some(t.to_owned()),
                None => {
                    page_token = None;
                    complete = true;
                    break;
                }
            }
        }

        // A completed incremental scan remains complete when it reached the
        // first known newest message. If it filled the entire page budget
        // without finding one, continue from the returned token next run.
        if !stopped_at_known && page_token.is_some() {
            complete = false;
        }

        new_ids.reverse(); // oldest first
        Ok((new_ids, page_token, complete))
    }

    /// Collect unknown ids, optionally stopping at the first known newest id.
    fn collect_page_ids(
        messages: &Value,
        known: &HashSet<String>,
        stop_at_known: bool,
        new_ids: &mut Vec<String>,
    ) -> bool {
        for message in messages.as_array().unwrap_or(&Vec::new()) {
            let Some(id) = message["id"].as_str() else {
                continue;
            };
            if known.contains(id) {
                if stop_at_known {
                    return true;
                }
                continue;
            }
            new_ids.push(id.to_owned());
        }
        false
    }

    fn has_label(labels: &Value, label: &str) -> bool {
        labels
            .as_array()
            .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(label)))
    }

    fn flags_from_labels(labels: &Value) -> (bool, bool) {
        (
            !Self::has_label(labels, "UNREAD"),
            Self::has_label(labels, "STARRED"),
        )
    }

    fn is_quota_error(error: &ProviderError) -> bool {
        match error {
            ProviderError::Http { status: 429, .. } => true,
            ProviderError::Http { status: 403, body } => {
                body.to_ascii_lowercase().contains("quota exceeded")
            }
            _ => false,
        }
    }

    fn fetched_metadata_from_value(uid: u32, res: &Value) -> Result<FetchedMessage, ProviderError> {
        let (is_seen, is_flagged) = Self::flags_from_labels(&res["labelIds"]);
        let internal_date = rfc3339_from_millis(
            res["internalDate"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
        );

        let empty = Vec::new();
        let headers = res["payload"]["headers"].as_array().unwrap_or(&empty);
        let block = header_block_from_pairs(
            headers
                .iter()
                .filter_map(|h| Some((h["name"].as_str()?, h["value"].as_str()?))),
        );

        Ok(fetched_from_raw(
            uid,
            &block,
            internal_date,
            is_seen,
            is_flagged,
            false,
        ))
    }

    async fn fetch_metadata_with_rest(
        rest: &Rest,
        uid: u32,
        remote_id: &str,
    ) -> Result<FetchedMessage, ProviderError> {
        let response = rest
            .get_json(&format!(
                "{BASE}/messages/{remote_id}?format=metadata&metadataHeaders=Subject&metadataHeaders=From&metadataHeaders=To&metadataHeaders=Cc&metadataHeaders=Date&metadataHeaders=Message-ID&metadataHeaders=Message-Id&metadataHeaders=In-Reply-To&metadataHeaders=References&metadataHeaders=List-Id&fields=id,labelIds,internalDate,payload/headers"
            ))
            .await?;
        Self::fetched_metadata_from_value(uid, &response)
    }

    async fn fetch_raw_message(&self, remote_id: &str) -> Result<(Vec<u8>, Value), ProviderError> {
        Self::fetch_raw_message_with_rest(&self.rest, remote_id).await
    }

    async fn fetch_raw_message_with_rest(
        rest: &Rest,
        remote_id: &str,
    ) -> Result<(Vec<u8>, Value), ProviderError> {
        let res = rest
            .get_json(&format!(
                "{BASE}/messages/{remote_id}?format=raw&fields=id,raw,labelIds,internalDate"
            ))
            .await?;
        Self::raw_message_from_value(res)
    }

    fn raw_message_from_value(res: Value) -> Result<(Vec<u8>, Value), ProviderError> {
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
        let res = self
            .rest
            .get_json(&format!("{BASE}/labels?fields=labels(id,name,type)"))
            .await?;
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
            .get_json(&format!(
                "{BASE}/labels/{}?fields=messagesTotal",
                urlencoding::encode(folder)
            ))
            .await?;
        Ok(FolderStatus {
            // The uid mapping is locally owned — no server-side generation marker.
            uidvalidity: 1,
            exists: res["messagesTotal"].as_u64().unwrap_or(0) as u32,
        })
    }

    async fn highest_uid(&mut self, folder: &str) -> Result<Option<u32>, ProviderError> {
        let (new_ids, next_page_token, backfill_complete) = self.discover_new(folder).await?;
        let max = if new_ids.is_empty() {
            self.ids.max_uid(folder).await?
        } else {
            self.ids.assign(folder, &new_ids).await?
        };
        // Advance only after every discovered id has been durably assigned.
        self.ids
            .set_backfill_state(folder, next_page_token.as_deref(), backfill_complete)
            .await?;
        Ok((max > 0).then_some(max))
    }

    async fn fetch_flags(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<(u32, bool, bool, bool)>, ProviderError> {
        let resolved = self.ids.resolve_set(folder, uid_set).await?;
        let rest = self.rest.clone();
        let results = stream::iter(resolved)
            .map(|(uid, remote_id)| {
                let rest = rest.clone();
                async move {
                    let result = rest
                        .get_json(&format!(
                            "{BASE}/messages/{remote_id}?format=minimal&fields=id,labelIds"
                        ))
                        .await;
                    (uid, remote_id, result)
                }
            })
            .buffer_unordered(CONCURRENT_FETCHES)
            .collect::<Vec<_>>()
            .await;

        let mut out = Vec::new();
        for (uid, _remote_id, res) in results {
            match res {
                Ok(v) => {
                    let (seen, flagged) = Self::flags_from_labels(&v["labelIds"]);
                    let deleted = !Self::has_label(&v["labelIds"], folder);
                    out.push((uid, seen, flagged, deleted));
                    if deleted {
                        self.ids.remove(folder, uid).await?;
                    }
                }
                Err(e) if Self::is_quota_error(&e) => return Err(e),
                Err(ProviderError::Http { status: 404, .. }) => {
                    out.push((uid, false, false, true));
                    self.ids.remove(folder, uid).await?;
                }
                // Authentication, transport, and provider failures are not
                // evidence that the message was deleted.
                Err(error) => return Err(error),
            }
        }
        Ok(out)
    }

    async fn fetch_headers(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<FetchedMessage>, ProviderError> {
        let resolved = self.ids.resolve_set(folder, uid_set).await?;
        let rest = self.rest.clone();
        let results = stream::iter(resolved)
            .map(|(uid, remote_id)| {
                let rest = rest.clone();
                async move {
                    let result = Self::fetch_metadata_with_rest(&rest, uid, &remote_id).await;
                    (remote_id, result)
                }
            })
            .buffer_unordered(CONCURRENT_FETCHES)
            .collect::<Vec<_>>()
            .await;

        let mut out = Vec::new();
        for (remote_id, result) in results {
            match result {
                Ok(m) => out.push(m),
                Err(e) if Self::is_quota_error(&e) => return Err(e),
                Err(e) => tracing::warn!(
                    "gmail: metadata fetch failed: account={} folder={folder} uid_set={uid_set} remote_id={remote_id} err={e}",
                    self.account_id
                ),
            }
        }
        Ok(out)
    }

    async fn fetch_full(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<FetchedMessage>, ProviderError> {
        let resolved = self.ids.resolve_set(folder, uid_set).await?;
        let rest = self.rest.clone();
        let results = stream::iter(resolved)
            .map(|(uid, remote_id)| {
                let rest = rest.clone();
                async move {
                    let result = Self::fetch_raw_message_with_rest(&rest, &remote_id).await;
                    (uid, remote_id, result)
                }
            })
            .buffer_unordered(CONCURRENT_FETCHES)
            .collect::<Vec<_>>()
            .await;

        let mut out = Vec::new();
        for (uid, remote_id, result) in results {
            match result {
                Ok((raw, meta)) => {
                    let (is_seen, is_flagged) = Self::flags_from_labels(&meta["labelIds"]);
                    let internal_date = rfc3339_from_millis(
                        meta["internalDate"]
                            .as_str()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0),
                    );
                    out.push(fetched_from_raw(
                        uid,
                        &raw,
                        internal_date,
                        is_seen,
                        is_flagged,
                        true,
                    ));
                }
                Err(e) if Self::is_quota_error(&e) => return Err(e),
                Err(e) => tracing::warn!(
                    "gmail: raw fetch failed: account={} folder={folder} uid_set={uid_set} uid={uid} remote_id={remote_id} err={e}",
                    self.account_id
                ),
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
            self.modify_labels(&remote_id, &[dest_folder], &[src_folder])
                .await?;
        }
        // Re-discovered with a fresh uid in the destination label.
        self.ids.remove(src_folder, uid).await
    }

    async fn delete_permanently(&mut self, folder: &str, uid: u32) -> Result<(), ProviderError> {
        let remote_id = self.ids.remote_id(folder, uid).await?;
        self.rest
            .delete(&format!("{BASE}/messages/{remote_id}"))
            .await?;
        self.ids.remove(folder, uid).await
    }

    async fn append_sent(&mut self, _raw_message: &[u8]) -> Result<(), ProviderError> {
        // Gmail stores sent mail itself when sending via the API.
        Ok(())
    }

    async fn send_message(&mut self, raw_message: &[u8]) -> Result<(), ProviderError> {
        let raw = Self::encode_raw_message(raw_message);
        self.rest
            .post_json(&format!("{BASE}/messages/send"), &json!({ "raw": raw }))
            .await?;
        Ok(())
    }

    async fn close(&mut self) -> Result<(), ProviderError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::GmailProvider;
    use serde_json::json;
    use std::collections::HashSet;

    #[test]
    fn flags_from_labels_maps_gmail_unread_label_to_seen_state() {
        let (seen, flagged) = GmailProvider::flags_from_labels(&json!(["INBOX", "STARRED"]));
        assert!(seen);
        assert!(flagged);

        let (seen, flagged) = GmailProvider::flags_from_labels(&json!(["INBOX", "UNREAD"]));
        assert!(!seen);
        assert!(!flagged);
    }

    #[test]
    fn flags_from_labels_defaults_missing_labels_to_read() {
        let (seen, flagged) = GmailProvider::flags_from_labels(&json!(null));
        assert!(seen);
        assert!(!flagged);
    }

    #[test]
    fn incomplete_backfill_skips_known_ids_and_keeps_scanning_older_mail() {
        let known = HashSet::from(["known-newest".to_owned()]);
        let mut discovered = Vec::new();

        let stopped = GmailProvider::collect_page_ids(
            &json!([{ "id": "known-newest" }, { "id": "older-undiscovered" }]),
            &known,
            false,
            &mut discovered,
        );

        assert!(!stopped);
        assert_eq!(discovered, ["older-undiscovered"]);
    }

    #[test]
    fn completed_backfill_stops_at_first_known_newest_id() {
        let known = HashSet::from(["known".to_owned()]);
        let mut discovered = Vec::new();

        let stopped = GmailProvider::collect_page_ids(
            &json!([{ "id": "new" }, { "id": "known" }, { "id": "old" }]),
            &known,
            true,
            &mut discovered,
        );

        assert!(stopped);
        assert_eq!(discovered, ["new"]);
    }

    #[test]
    fn quota_errors_are_detected_from_gmail_http_body() {
        let error = super::ProviderError::Http {
            status: 403,
            body: "Quota exceeded for quota metric 'Queries'".to_owned(),
        };
        assert!(GmailProvider::is_quota_error(&error));

        let forbidden = super::ProviderError::Http {
            status: 403,
            body: "The caller does not have permission".to_owned(),
        };
        assert!(!GmailProvider::is_quota_error(&forbidden));

        assert!(GmailProvider::is_quota_error(&super::ProviderError::Http {
            status: 429,
            body: "rate limited".to_owned(),
        }));
    }

    #[test]
    fn encode_raw_message_uses_unpadded_base64url() {
        assert_eq!(GmailProvider::encode_raw_message(b"\xfb\xff"), "-_8");
    }
}
