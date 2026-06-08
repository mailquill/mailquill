use std::sync::Arc;

use imap_sync::manager::{NewMessageNotification, SyncAppState, SyncManager};
use mailquill_core::{blob::BlobStore, crypto::CredentialKey};
use web_push::{
    ContentEncoding, SubscriptionInfo, Urgency, VapidSignatureBuilder, WebPushClient,
    WebPushMessageBuilder,
};

use crate::state::AppState;

#[async_trait::async_trait]
impl SyncAppState for AppState {
    async fn user_db(&self, user_id: &str) -> Result<sqlx::SqlitePool, String> {
        self.user_db_pool.get(user_id).await.map_err(|e| e.to_string())
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

    async fn notify_new_message(
        &self,
        user_id: &str,
        message: NewMessageNotification,
    ) -> Result<(), String> {
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

        let payload = crate::routes::push_subscriptions::push_payload(
            &message.message_id,
            &message.account_id,
            &message.account_name,
            &message.sender,
            &message.subject,
        )
        .map_err(|e| e.to_string())?;

        for (id, endpoint, p256dh, auth) in subscriptions {
            let subscription = SubscriptionInfo::new(&endpoint, &p256dh, &auth);
            let mut signature_builder =
                VapidSignatureBuilder::from_base64(&vapid.private_key, &subscription)
                    .map_err(|e| e.to_string())?;
            signature_builder.add_claim("sub", vapid.subject.clone());
            let signature = signature_builder.build().map_err(|e| e.to_string())?;

            let mut builder = WebPushMessageBuilder::new(&subscription);
            builder.set_payload(ContentEncoding::Aes128Gcm, &payload);
            builder.set_ttl(300);
            builder.set_urgency(Urgency::Normal);
            builder.set_vapid_signature(signature);

            if let Err(e) = client
                .send(builder.build().map_err(|e| e.to_string())?)
                .await
            {
                let error = e.to_string();
                if error.contains("410") || error.to_ascii_lowercase().contains("gone") {
                    crate::routes::push_subscriptions::delete_stale_subscription(self, &id).await;
                } else {
                    tracing::warn!("web push delivery failed: {error}");
                }
            }
        }

        Ok(())
    }
}
