use axum::{
    extract::{Extension, Path, State},
    response::IntoResponse,
    Json,
};

use crate::{error::AppError, middleware::UserId, state::AppState};

/// Build per-protocol auth from decrypted account credentials. Prefers an OAuth
/// bearer token; falls back to IMAP basic credentials.
fn build_auth(creds: &serde_json::Value, email: &str) -> (contact_sync::DavAuth, calendar_sync::DavAuth) {
    if let Some(token) = creds["oauth_access_token"].as_str() {
        return (
            contact_sync::DavAuth::Bearer(token.to_owned()),
            calendar_sync::DavAuth::Bearer(token.to_owned()),
        );
    }
    let user = creds["imap_username"].as_str().filter(|s| !s.is_empty()).unwrap_or(email).to_owned();
    let pass = creds["imap_password"].as_str().unwrap_or("").to_owned();
    (
        contact_sync::DavAuth::Basic { username: user.clone(), password: pass.clone() },
        calendar_sync::DavAuth::Basic { username: user, password: pass },
    )
}

fn domain_of(email: &str) -> &str {
    email.split('@').nth(1).unwrap_or("localhost")
}

/// Sync contacts (CardDAV) and calendar events (CalDAV) for one account into the
/// local tables. Each protocol is attempted independently; failures are reported
/// rather than aborting the whole request.
pub async fn sync_dav(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    let row: Option<(String, Vec<u8>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT primary_email, credentials_encrypted, carddav_url, caldav_url FROM email_accounts WHERE id = ?",
    )
    .bind(&account_id)
    .fetch_optional(&user_db)
    .await?;
    let (email, creds_enc, carddav_url, caldav_url) = row.ok_or(AppError::NotFound)?;

    let creds_bytes = state
        .credential_key
        .decrypt(&creds_enc)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let creds: serde_json::Value =
        serde_json::from_slice(&creds_bytes).map_err(|e| AppError::Internal(e.to_string()))?;
    let (card_auth, cal_auth) = build_auth(&creds, &email);

    let domain = domain_of(&email).to_owned();
    let carddav = carddav_url.unwrap_or_else(|| format!("https://{domain}/.well-known/carddav"));
    let caldav = caldav_url.unwrap_or_else(|| format!("https://{domain}/.well-known/caldav"));

    let mut contacts_synced = 0usize;
    let mut events_synced = 0usize;
    let mut errors: Vec<String> = Vec::new();

    // ── contacts ──
    match contact_sync::sync_carddav(&carddav, &card_auth).await {
        Ok(contacts) => {
            sqlx::query("DELETE FROM contacts WHERE account_id = ?")
                .bind(&account_id)
                .execute(&user_db)
                .await?;
            for c in &contacts {
                sqlx::query(
                    "INSERT INTO contacts (account_id, display_name, email, phone, company, job_title, favorite) VALUES (?, ?, ?, ?, ?, ?, 0)",
                )
                .bind(&account_id)
                .bind(&c.display_name)
                .bind(&c.email)
                .bind(&c.phone)
                .bind(&c.company)
                .bind(&c.job_title)
                .execute(&user_db)
                .await?;
            }
            contacts_synced = contacts.len();
        }
        Err(e) => errors.push(format!("carddav: {e}")),
    }

    // ── calendar ──
    match calendar_sync::sync_caldav(&caldav, &cal_auth).await {
        Ok(events) => {
            // ensure a calendar row for this account, then replace its events
            let cal_id: Option<String> =
                sqlx::query_scalar("SELECT id FROM calendars WHERE account_id = ? LIMIT 1")
                    .bind(&account_id)
                    .fetch_optional(&user_db)
                    .await?;
            let cal_id = match cal_id {
                Some(id) => id,
                None => sqlx::query_scalar(
                    "INSERT INTO calendars (account_id, name, color) VALUES (?, ?, '#2563EB') RETURNING id",
                )
                .bind(&account_id)
                .bind(&email)
                .fetch_one(&user_db)
                .await?,
            };
            sqlx::query("DELETE FROM calendar_events WHERE calendar_id = ?")
                .bind(&cal_id)
                .execute(&user_db)
                .await?;
            for ev in &events {
                sqlx::query(
                    "INSERT INTO calendar_events (calendar_id, title, description, location, starts_at, ends_at, all_day) VALUES (?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&cal_id)
                .bind(&ev.title)
                .bind(&ev.description)
                .bind(&ev.location)
                .bind(&ev.starts_at)
                .bind(&ev.ends_at)
                .bind(ev.all_day)
                .execute(&user_db)
                .await?;
            }
            events_synced = events.len();
        }
        Err(e) => errors.push(format!("caldav: {e}")),
    }

    Ok(Json(serde_json::json!({
        "contacts": contacts_synced,
        "events": events_synced,
        "errors": errors,
    })))
}
