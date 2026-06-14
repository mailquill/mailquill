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

    // ── calendar (two-way) ──
    match calendar_sync::discover_collection(&caldav, &cal_auth).await {
        Ok(collection) => {
            // Ensure a calendar row for this account and record its collection URL.
            let cal_id: String = match sqlx::query_scalar::<_, String>(
                "SELECT id FROM calendars WHERE account_id = ? LIMIT 1",
            )
            .bind(&account_id)
            .fetch_optional(&user_db)
            .await?
            {
                Some(id) => {
                    sqlx::query("UPDATE calendars SET dav_url = ? WHERE id = ?")
                        .bind(&collection)
                        .bind(&id)
                        .execute(&user_db)
                        .await?;
                    id
                }
                None => sqlx::query_scalar(
                    "INSERT INTO calendars (account_id, name, color, dav_url) VALUES (?, ?, '#2563EB', ?) RETURNING id",
                )
                .bind(&account_id)
                .bind(&email)
                .bind(&collection)
                .fetch_one(&user_db)
                .await?,
            };

            match calendar_sync::pull(&collection, &cal_auth).await {
                Ok(server_events) => {
                    match merge_calendar(&user_db, &cal_id, &collection, &cal_auth, server_events).await {
                        Ok((n, mut errs)) => {
                            events_synced = n;
                            errors.append(&mut errs);
                        }
                        Err(e) => errors.push(format!("caldav merge: {e}")),
                    }
                }
                Err(e) => errors.push(format!("caldav pull: {e}")),
            }
        }
        Err(e) => errors.push(format!("caldav discover: {e}")),
    }

    Ok(Json(serde_json::json!({
        "contacts": contacts_synced,
        "events": events_synced,
        "errors": errors,
    })))
}

/// One local calendar row, in the column order merge_calendar selects.
type LocalEvent = (
    String,         // id
    Option<String>, // uid
    Option<String>, // href
    Option<String>, // etag
    i64,            // dirty
    i64,            // deleted
    String,         // title
    Option<String>, // description
    Option<String>, // location
    String,         // starts_at
    String,         // ends_at
    bool,           // all_day
);

/// Reconcile a calendar's local rows with the server's events (ETag-based,
/// non-destructive). Pulls remote changes into non-dirty rows, pushes dirty
/// rows (PUT), propagates local deletes (tombstones → DELETE), and drops rows
/// the server no longer has. Push failures are collected, not fatal.
async fn merge_calendar(
    db: &sqlx::SqlitePool,
    cal_id: &str,
    collection: &str,
    auth: &calendar_sync::DavAuth,
    server: Vec<calendar_sync::ParsedEvent>,
) -> Result<(usize, Vec<String>), AppError> {
    use std::collections::HashSet;
    let mut errors = Vec::new();

    let local: Vec<LocalEvent> = sqlx::query_as(
        "SELECT id, uid, href, etag, dirty, deleted, title, description, location, starts_at, ends_at, all_day \
         FROM calendar_events WHERE calendar_id = ?",
    )
    .bind(cal_id)
    .fetch_all(db)
    .await?;

    let server_uids: HashSet<&str> =
        server.iter().map(|e| e.uid.as_str()).filter(|u| !u.is_empty()).collect();

    // 1. Server -> local: update non-dirty changed rows, insert new ones.
    for se in &server {
        if se.uid.is_empty() {
            continue;
        }
        match local.iter().find(|l| l.1.as_deref() == Some(se.uid.as_str())) {
            Some(l) => {
                if l.4 == 0 && l.5 == 0 && l.3.as_deref() != se.etag.as_deref() {
                    sqlx::query(
                        "UPDATE calendar_events SET title=?, description=?, location=?, starts_at=?, ends_at=?, all_day=?, href=?, etag=? WHERE id=?",
                    )
                    .bind(&se.title).bind(&se.description).bind(&se.location)
                    .bind(&se.starts_at).bind(&se.ends_at).bind(se.all_day)
                    .bind(&se.href).bind(&se.etag).bind(&l.0)
                    .execute(db).await?;
                }
            }
            None => {
                sqlx::query(
                    "INSERT INTO calendar_events (calendar_id, uid, href, etag, title, description, location, starts_at, ends_at, all_day, dirty, deleted) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0)",
                )
                .bind(cal_id).bind(&se.uid).bind(&se.href).bind(&se.etag)
                .bind(&se.title).bind(&se.description).bind(&se.location)
                .bind(&se.starts_at).bind(&se.ends_at).bind(se.all_day)
                .execute(db).await?;
            }
        }
    }

    // 2. Local -> server: tombstones, dirty pushes, server-side deletions.
    for l in &local {
        if l.5 == 1 {
            // Tombstone: delete on the server, then remove locally. Keep it for a
            // later retry if the server delete fails.
            if let Some(href) = &l.2 {
                if let Err(e) = calendar_sync::delete_event(href, auth, l.3.as_deref()).await {
                    errors.push(format!("caldav delete: {e}"));
                    continue;
                }
            }
            sqlx::query("DELETE FROM calendar_events WHERE id = ?").bind(&l.0).execute(db).await?;
            continue;
        }

        if l.4 == 1 {
            // Dirty: push as create (no href) or update (If-Match etag).
            let uid = l.1.clone().unwrap_or_else(|| format!("{}@mailquill", uuid::Uuid::new_v4()));
            let resource = l
                .2
                .clone()
                .unwrap_or_else(|| format!("{}/{}.ics", collection.trim_end_matches('/'), uid));
            let ics = calendar_sync::build_ics(&calendar_sync::EventInput {
                uid: &uid,
                title: &l.6,
                starts_at: &l.9,
                ends_at: &l.10,
                all_day: l.11,
                location: l.8.as_deref(),
                description: l.7.as_deref(),
            });
            match calendar_sync::put_event(&resource, auth, &ics, l.3.as_deref()).await {
                Ok(new_etag) => {
                    sqlx::query("UPDATE calendar_events SET dirty=0, uid=?, href=?, etag=? WHERE id=?")
                        .bind(&uid).bind(&resource).bind(&new_etag).bind(&l.0)
                        .execute(db).await?;
                }
                Err(e) => errors.push(format!("caldav put: {e}")),
            }
        } else if let Some(uid) = &l.1 {
            // Clean row that had a server copy but is gone from the server now —
            // the remote delete wins.
            if l.2.is_some() && !server_uids.contains(uid.as_str()) {
                sqlx::query("DELETE FROM calendar_events WHERE id = ?").bind(&l.0).execute(db).await?;
            }
        }
    }

    Ok((server.len(), errors))
}
