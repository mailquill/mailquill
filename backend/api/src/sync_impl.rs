use std::sync::Arc;

use mail_sync::manager::{
    NewMessageNotification, SyncAppState, SyncManager, SyncStatusNotification,
};
use mailquill_core::{blob::BlobStore, crypto::CredentialKey};
use web_push_native::{
    jwt_simple::algorithms::ES256KeyPair, p256::PublicKey, Auth, WebPushBuilder,
};

/// Browser push subscriptions encode `p256dh`/`auth` as URL-safe base64;
/// padding varies by client, so accept both padded and unpadded input.
const B64URL: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
    &base64::alphabet::URL_SAFE,
    base64::engine::GeneralPurposeConfig::new()
        .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent),
);

use crate::state::AppState;

#[async_trait::async_trait]
impl SyncAppState for AppState {
    async fn user_db(&self, user_id: &str) -> Result<sqlx::SqlitePool, String> {
        self.user_db_pool
            .get(user_id)
            .await
            .map_err(|e| e.to_string())
    }

    fn blob_store(&self) -> Arc<dyn BlobStore> {
        self.blob_store.clone()
    }

    fn credential_key(&self) -> Arc<CredentialKey> {
        self.credential_key.clone()
    }

    fn sync_manager(&self) -> Arc<SyncManager> {
        self.sync_manager.clone()
    }

    async fn fresh_oauth_token(
        &self,
        user_id: &str,
        account_id: &str,
    ) -> Result<Option<String>, String> {
        let user_db = self
            .user_db_pool
            .get(user_id)
            .await
            .map_err(|error| error.to_string())?;
        crate::oauth_tokens::fresh_access_token(&self.credential_key, &user_db, account_id).await
    }

    async fn notify_new_message(
        &self,
        user_id: &str,
        message: NewMessageNotification,
    ) -> Result<(), String> {
        let payload = crate::routes::push_subscriptions::push_payload(
            &message.message_id,
            &message.account_id,
            &message.account_name,
            &message.sender,
            &message.subject,
        )
        .map_err(|e| e.to_string())?;

        // Foreground SSE: always broadcast (open clients show a notification
        // even without web push). Errors mean no subscriber — ignore.
        let _ = self.events.send(crate::state::UserEvent {
            user_id: user_id.to_owned(),
            event_type: "message".to_owned(),
            payload: String::from_utf8_lossy(&payload).into_owned(),
        });

        // Background web push, only when VAPID is configured.
        let Some(vapid) = &self.vapid else {
            return Ok(());
        };
        let Some(client) = &self.web_push_client else {
            return Ok(());
        };

        let subscriptions: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT id, endpoint, p256dh, auth FROM push_subscriptions WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_all(&self.app_db)
        .await
        .map_err(|e| e.to_string())?;

        if subscriptions.is_empty() {
            return Ok(());
        }

        let key_pair = match base64::Engine::decode(&B64URL, &vapid.private_key)
            .map_err(|e| e.to_string())
            .and_then(|bytes| ES256KeyPair::from_bytes(&bytes).map_err(|e| e.to_string()))
        {
            Ok(kp) => kp,
            Err(e) => {
                tracing::warn!("web push disabled: invalid VAPID private key: {e}");
                return Ok(());
            }
        };

        for (id, endpoint, p256dh, auth) in subscriptions {
            // A subscription that cannot be parsed can never be delivered to —
            // treat it like a stale one instead of retrying forever.
            let parsed = endpoint
                .parse::<axum::http::Uri>()
                .map_err(|e| e.to_string())
                .and_then(|uri| {
                    let public = base64::Engine::decode(&B64URL, &p256dh)
                        .map_err(|e| e.to_string())
                        .and_then(|b| {
                            PublicKey::from_sec1_bytes(&b).map_err(|e| e.to_string())
                        })?;
                    let auth_bytes =
                        base64::Engine::decode(&B64URL, &auth).map_err(|e| e.to_string())?;
                    if auth_bytes.len() != Auth::default().len() {
                        return Err("auth secret must be 16 bytes".into());
                    }
                    Ok((uri, public, Auth::clone_from_slice(&auth_bytes)))
                });
            let (uri, ua_public, ua_auth) = match parsed {
                Ok(parts) => parts,
                Err(e) => {
                    tracing::warn!("dropping unparsable push subscription: {e}");
                    crate::routes::push_subscriptions::delete_stale_subscription(self, &id).await;
                    continue;
                }
            };

            let request = match WebPushBuilder::new(uri, ua_public, ua_auth)
                .with_valid_duration(std::time::Duration::from_secs(300))
                .with_vapid(&key_pair, &vapid.subject)
                .build(payload.clone())
                .map_err(|e| e.to_string())
                .and_then(|req| reqwest::Request::try_from(req).map_err(|e| e.to_string()))
            {
                Ok(req) => req,
                Err(e) => {
                    tracing::warn!("web push request build failed: {e}");
                    continue;
                }
            };

            match client.execute(request).await {
                Ok(resp) if resp.status() == 404 || resp.status() == 410 => {
                    crate::routes::push_subscriptions::delete_stale_subscription(self, &id).await;
                }
                Ok(resp) if !resp.status().is_success() => {
                    tracing::warn!("web push delivery failed: HTTP {}", resp.status());
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("web push delivery failed: {e}"),
            }
        }

        Ok(())
    }

    async fn notify_sync_status(
        &self,
        user_id: &str,
        status: SyncStatusNotification,
    ) -> Result<(), String> {
        let payload = serde_json::to_string(&status).map_err(|error| error.to_string())?;
        let _ = self.events.send(crate::state::UserEvent {
            user_id: user_id.to_owned(),
            event_type: "sync".to_owned(),
            payload,
        });
        Ok(())
    }
}
