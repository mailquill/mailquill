pub mod manager;
pub mod mime;
pub mod session;
pub mod sync;
pub mod threading;

use std::sync::Arc;
use mailquill_core::blob::BlobStore;

pub use session::SessionError;

/// Test IMAP connectivity with given credentials (used during account creation).
pub async fn test_imap_connection(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    auth_scheme: &str,
) -> Result<(), SessionError> {
    let mut sess = session::connect_imap(host, port, username, password, None, auth_scheme).await?;
    let _ = sess.logout().await;
    Ok(())
}

/// On-demand body fetch for a specific message (task 4.10).
/// Returns (html_body, text_body).
pub async fn fetch_body_by_uid(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    oauth_token: Option<&str>,
    auth_scheme: &str,
    folder_path: &str,
    uid: u32,
    blob_store: Arc<dyn BlobStore>,
    account_id: &str,
    folder_id: &str,
    message_id: &str,
    _user_id: &str,
    user_db: &sqlx::SqlitePool,
) -> Result<(Option<String>, Option<String>), Box<dyn std::error::Error + Send + Sync>> {
    let mut sess = session::connect_imap(host, port, username, password, oauth_token, auth_scheme)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    session::select_folder(&mut sess, folder_path)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    let raw = session::fetch_body_uid(&mut sess, uid)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    let _ = sess.logout().await;

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
    blob_store.put(&blob_key, blob).await.map_err(|e| e.to_string())?;

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
    raw_message: &[u8],
) -> Result<(), SessionError> {
    let mut sess = session::connect_imap(host, port, username, password, oauth_token, auth_scheme).await?;
    session::append_to_sent(&mut sess, raw_message).await?;
    let _ = sess.logout().await;
    Ok(())
}
