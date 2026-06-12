pub mod manager;
pub mod mime;
pub mod provider;
pub mod session;
pub mod sync;
pub mod threading;

use mailquill_core::blob::BlobStore;
use std::sync::Arc;

pub use session::SessionError;

/// Test IMAP connectivity with given credentials (used during account creation).
pub async fn test_imap_connection(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    auth_scheme: &str,
    trusted_cert_der: Option<&[u8]>,
) -> Result<(), SessionError> {
    let mut sess = session::connect_imap(
        host,
        port,
        username,
        password,
        None,
        auth_scheme,
        trusted_cert_der,
    )
    .await?;
    let _ = sess.logout().await;
    Ok(())
}

/// On-demand body fetch for a specific message (task 4.10), via the account's
/// configured backend. Returns (html_body, text_body).
#[allow(clippy::too_many_arguments)]
pub async fn fetch_body_by_uid(
    kind: provider::ProviderKind,
    config: &provider::ProviderConfig,
    folder_path: &str,
    uid: u32,
    blob_store: Arc<dyn BlobStore>,
    account_id: &str,
    folder_id: &str,
    message_id: &str,
    user_db: &sqlx::SqlitePool,
) -> Result<(Option<String>, Option<String>), Box<dyn std::error::Error + Send + Sync>> {
    let mut prov = provider::connect(kind, config)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    let raw = prov
        .fetch_raw(folder_path, uid)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    let _ = prov.close().await;

    let parsed = mime::parse_mime(&raw).map_err(|e| e.to_string())?;

    // Store body in blob store
    let body_json = serde_json::json!({
        "html": parsed.html,
        "text": parsed.text,
    });
    let body_bytes = serde_json::to_vec(&body_json).map_err(|e| e.to_string())?;
    let compressed = mailquill_core::compression::compress_body(&body_bytes);

    let internal_date = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let blob_key = mailquill_core::blob::blob_key_body(account_id, folder_id, uid, internal_date);

    let blob = bytes::Bytes::from(compressed.clone());
    blob_store
        .put(&blob_key, blob)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query(
        "INSERT OR IGNORE INTO message_bodies (message_id, blob_key, size_bytes, size_bytes_uncompressed) VALUES (?, ?, ?, ?)",
    )
    .bind(message_id)
    .bind(&blob_key)
    .bind(compressed.len() as i64)
    .bind(body_bytes.len() as i64)
    .execute(user_db)
    .await
    .map_err(|e| e.to_string())?;

    // Store attachments (lazy-synced messages only get them here; full sync
    // stores them during the folder walk). Skip if already recorded.
    let have_attachments: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM attachments WHERE message_id = ?")
            .bind(message_id)
            .fetch_one(user_db)
            .await
            .unwrap_or(0);
    if have_attachments == 0 {
        for (i, att) in parsed.attachments.iter().enumerate() {
            let att_key = mailquill_core::blob::blob_key_attachment(
                account_id,
                folder_id,
                uid,
                internal_date,
                i,
            );
            let att_blob = bytes::Bytes::from(att.data.clone());
            if blob_store.put(&att_key, att_blob).await.is_ok() {
                let _ = sqlx::query(
                    "INSERT OR IGNORE INTO attachments (message_id, filename, content_type, content_id, size_bytes, blob_key) VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(message_id)
                .bind(att.filename.as_deref())
                .bind(&att.content_type)
                .bind(att.content_id.as_deref())
                .bind(att.data.len() as i64)
                .bind(&att_key)
                .execute(user_db)
                .await;
            }
        }
    }

    // Update FTS index
    let body_text = parsed.text.as_deref().unwrap_or("");
    if !body_text.is_empty() {
        let _ = sqlx::query(
            "INSERT INTO messages_fts(rowid, subject, from_addr, body_text) VALUES ((SELECT rowid FROM messages WHERE id = ?), (SELECT subject FROM messages WHERE id = ?), (SELECT from_addr FROM messages WHERE id = ?), ?) ON CONFLICT DO UPDATE SET body_text = excluded.body_text",
        )
        .bind(message_id)
        .bind(message_id)
        .bind(message_id)
        .bind(body_text)
        .execute(user_db)
        .await;
    }

    // Phishing analysis needs the raw message (headers + body) — this is the
    // only place lazy-synced messages have it.
    phishing::analyse_and_store(user_db, message_id, &raw).await;

    Ok((parsed.html, parsed.text))
}

/// Append raw RFC 2822 message bytes to the IMAP Sent folder.
pub async fn append_to_sent(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    oauth_token: Option<&str>,
    auth_scheme: &str,
    trusted_cert_der: Option<&[u8]>,
    raw_message: &[u8],
) -> Result<(), SessionError> {
    let mut sess = session::connect_imap(
        host,
        port,
        username,
        password,
        oauth_token,
        auth_scheme,
        trusted_cert_der,
    )
    .await?;
    session::append_to_sent(&mut sess, raw_message).await?;
    let _ = sess.logout().await;
    Ok(())
}
