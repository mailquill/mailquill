use axum::{
    extract::{Extension, Path, Query, State},
    response::IntoResponse,
    Json,
};
use serde::Serialize;

use crate::{error::AppError, middleware::UserId, state::AppState};

#[derive(Serialize)]
struct DiscoveredCalendarResponse {
    name: String,
    url: String,
    color: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct CaldavDiscoverQuery {
    accept_invalid_tls: Option<bool>,
    tls_decision: Option<crate::tls::TlsDecision>,
}

/// Build per-protocol auth from decrypted account credentials. Prefers an OAuth
/// bearer token; falls back to IMAP basic credentials.
fn build_auth(
    creds: &serde_json::Value,
    email: &str,
) -> (contact_sync::DavAuth, calendar_sync::DavAuth) {
    if let Some(token) = creds["oauth_access_token"].as_str() {
        return (
            contact_sync::DavAuth::Bearer(token.to_owned()),
            calendar_sync::DavAuth::Bearer(token.to_owned()),
        );
    }
    let user = creds["imap_username"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(email)
        .to_owned();
    let pass = creds["imap_password"].as_str().unwrap_or("").to_owned();
    (
        contact_sync::DavAuth::Basic {
            username: user.clone(),
            password: pass.clone(),
        },
        calendar_sync::DavAuth::Basic {
            username: user,
            password: pass,
        },
    )
}

fn domain_of(email: &str) -> &str {
    email.split('@').nth(1).unwrap_or("localhost")
}

async fn discover_caldav_candidates(
    email: &str,
    configured_url: Option<&str>,
    auth: &calendar_sync::DavAuth,
    trusted_cert_der: Option<&[u8]>,
    accept_invalid_tls: bool,
) -> Result<(String, Vec<calendar_sync::DiscoveredCalendar>), String> {
    let mut errors = Vec::new();
    for candidate in caldav_candidates(email, configured_url) {
        match calendar_sync::discover_collections_with_tls_options(
            &candidate,
            auth,
            trusted_cert_der,
            accept_invalid_tls,
        )
        .await
        {
            Ok(calendars) if !calendars.is_empty() => return Ok((candidate, calendars)),
            Ok(_) => errors.push(format!("{candidate}: no calendars found")),
            Err(err) => errors.push(format!("{candidate}: {err}")),
        }
    }
    Err(format!("caldav discover failed: {}", errors.join("; ")))
}

fn caldav_candidates(email: &str, configured_url: Option<&str>) -> Vec<String> {
    let domain = domain_of(email);
    let localpart = email.split('@').next().unwrap_or(email);
    let mut candidates = Vec::new();
    if let Some(url) = configured_url.map(str::trim).filter(|url| !url.is_empty()) {
        candidates.push(url.to_owned());
    }
    candidates.extend(provider_caldav_candidates(domain, email));
    candidates.extend([
        format!("https://{domain}/.well-known/caldav"),
        format!("https://{domain}/SOGo/dav/{email}/"),
        format!("https://{domain}/SOGo/dav/{email}/Calendar/"),
        format!("https://{domain}/SOGo/dav/{localpart}/"),
        format!("https://{domain}/SOGo/dav/{localpart}/Calendar/"),
        format!("https://mail.{domain}/.well-known/caldav"),
        format!("https://mail.{domain}/SOGo/dav/{email}/"),
        format!("https://mail.{domain}/SOGo/dav/{email}/Calendar/"),
        format!("https://mail.{domain}/SOGo/dav/{localpart}/"),
        format!("https://mail.{domain}/SOGo/dav/{localpart}/Calendar/"),
        format!("https://webmail.{domain}/.well-known/caldav"),
        format!("https://webmail.{domain}/SOGo/dav/{email}/"),
        format!("https://webmail.{domain}/SOGo/dav/{email}/Calendar/"),
        format!("https://webmail.{domain}/SOGo/dav/{localpart}/"),
        format!("https://webmail.{domain}/SOGo/dav/{localpart}/Calendar/"),
        format!("https://sogo.{domain}/.well-known/caldav"),
        format!("https://sogo.{domain}/SOGo/dav/{email}/"),
        format!("https://sogo.{domain}/SOGo/dav/{email}/Calendar/"),
        format!("https://sogo.{domain}/SOGo/dav/{localpart}/"),
        format!("https://sogo.{domain}/SOGo/dav/{localpart}/Calendar/"),
        format!("https://dav.{domain}/.well-known/caldav"),
        format!("https://dav.{domain}/caldav/"),
        format!("https://dav.{domain}/SOGo/dav/{email}/"),
        format!("https://dav.{domain}/SOGo/dav/{email}/Calendar/"),
        format!("https://dav.{domain}/SOGo/dav/{localpart}/"),
        format!("https://dav.{domain}/SOGo/dav/{localpart}/Calendar/"),
        format!("https://caldav.{domain}/"),
        format!("https://calendar.{domain}/.well-known/caldav"),
    ]);
    dedupe_urls(candidates)
}

fn provider_caldav_candidates(domain: &str, email: &str) -> Vec<String> {
    let encoded = urlencoding::encode(email);
    match domain {
        "gmail.com" | "googlemail.com" => vec![format!(
            "https://apidata.googleusercontent.com/caldav/v2/{encoded}/events/"
        )],
        "icloud.com" | "me.com" | "mac.com" => vec!["https://caldav.icloud.com/".to_owned()],
        "fastmail.com" | "fastmail.fm" => vec!["https://caldav.fastmail.com/".to_owned()],
        "gmx.net" | "gmx.de" => vec!["https://caldav.gmx.net/".to_owned()],
        "web.de" => vec!["https://caldav.web.de/".to_owned()],
        "mailbox.org" => vec!["https://dav.mailbox.org/".to_owned()],
        "posteo.de" => vec!["https://posteo.de:8443/".to_owned()],
        _ => Vec::new(),
    }
}

fn dedupe_urls(urls: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    urls.into_iter()
        .filter(|url| seen.insert(url.trim_end_matches('/').to_ascii_lowercase()))
        .collect()
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

    let row: Option<(String, Vec<u8>, Option<String>, Option<String>, Option<String>, Option<String>, bool)> = sqlx::query_as(
        "SELECT primary_email, credentials_encrypted, carddav_url, caldav_url, imap_tls_cert, smtp_tls_cert, caldav_accept_invalid_tls FROM email_accounts WHERE id = ?",
    )
    .bind(&account_id)
    .fetch_optional(&user_db)
    .await?;
    let (
        email,
        creds_enc,
        carddav_url,
        caldav_url,
        imap_tls_cert,
        smtp_tls_cert,
        accept_invalid_tls,
    ) = row.ok_or(AppError::NotFound)?;

    let creds_bytes = state
        .credential_key
        .decrypt(&creds_enc)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let creds: serde_json::Value =
        serde_json::from_slice(&creds_bytes).map_err(|e| AppError::Internal(e.to_string()))?;
    let (card_auth, cal_auth) = build_auth(&creds, &email);
    let trusted_cert_der = mail_sync::session::decode_trusted_cert(
        imap_tls_cert.as_deref().or(smtp_tls_cert.as_deref()),
    );

    let domain = domain_of(&email).to_owned();
    let carddav = carddav_url.unwrap_or_else(|| format!("https://{domain}/.well-known/carddav"));
    let caldav = caldav_url
        .clone()
        .unwrap_or_else(|| format!("https://{domain}/.well-known/caldav"));

    let mut contacts_synced = 0usize;
    let mut events_synced = 0usize;
    let mut errors: Vec<String> = Vec::new();

    // ── contacts ──
    match contact_sync::sync_carddav_with_tls_options(
        &carddav,
        &card_auth,
        trusted_cert_der.as_deref(),
        accept_invalid_tls,
    )
    .await
    {
        Ok(contacts) => {
            let auth_scheme = if matches!(card_auth, contact_sync::DavAuth::Bearer(_)) {
                "oauth2"
            } else {
                "basic"
            };
            sqlx::query(
                "INSERT INTO contact_accounts (id, display_name, type, base_url, auth_scheme, credentials_encrypted) \
                 VALUES (?, ?, 'cardav', ?, ?, ?) \
                 ON CONFLICT(id) DO UPDATE SET base_url=excluded.base_url, auth_scheme=excluded.auth_scheme, credentials_encrypted=excluded.credentials_encrypted",
            )
            .bind(&account_id)
            .bind(format!("{email} contacts"))
            .bind(&carddav)
            .bind(auth_scheme)
            .bind(&creds_enc)
            .execute(&user_db)
            .await?;
            sqlx::query("DELETE FROM contacts WHERE account_id = ?")
                .bind(&account_id)
                .execute(&user_db)
                .await?;
            for c in &contacts {
                let emails = serde_json::to_string(&c.emails)
                    .map_err(|e| AppError::Internal(e.to_string()))?;
                let phones = serde_json::to_string(&c.phones)
                    .map_err(|e| AppError::Internal(e.to_string()))?;
                let addresses = serde_json::to_string(&c.addresses)
                    .map_err(|e| AppError::Internal(e.to_string()))?;
                sqlx::query(
                    "INSERT INTO contacts \
                     (account_id, uid, display_name, given_name, family_name, org, title, emails, phones, addresses, notes, raw_vcard, synced_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))",
                )
                .bind(&account_id)
                .bind(&c.uid)
                .bind(&c.display_name)
                .bind(&c.given_name)
                .bind(&c.family_name)
                .bind(&c.org)
                .bind(&c.title)
                .bind(&emails)
                .bind(&phones)
                .bind(&addresses)
                .bind(&c.notes)
                .bind(&c.raw_vcard)
                .execute(&user_db)
                .await?;
            }
            contacts_synced = contacts.len();
        }
        Err(e) => errors.push(format!("carddav: {e}")),
    }

    // ── calendar (two-way, one or more collections) ──
    let mut cals: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, dav_url, ctag FROM calendars WHERE account_id = ? AND dav_url IS NOT NULL",
    )
    .bind(&account_id)
    .fetch_all(&user_db)
    .await?;
    // No CalDAV calendar configured yet — auto-discover the default collection.
    if cals.is_empty() {
        match discover_caldav_candidates(
            &email,
            caldav_url.as_deref(),
            &cal_auth,
            trusted_cert_der.as_deref(),
            accept_invalid_tls,
        )
        .await
        .map(|(_, calendars)| calendars)
        {
            Ok(collections) if !collections.is_empty() => {
                for calendar in collections {
                    let color = calendar.color.unwrap_or_else(|| "#2563EB".to_owned());
                    let id: String = sqlx::query_scalar(
                        "INSERT INTO calendars (account_id, name, color, dav_url) VALUES (?, ?, ?, ?) RETURNING id",
                    )
                    .bind(&account_id)
                    .bind(&calendar.name)
                    .bind(&color)
                    .bind(&calendar.url)
                    .fetch_one(&user_db)
                    .await?;
                    cals.push((id, calendar.url, None));
                }
            }
            Ok(_) => match calendar_sync::discover_collection(&caldav, &cal_auth).await {
                Ok(collection) => {
                    let id: String = sqlx::query_scalar(
                        "INSERT INTO calendars (account_id, name, color, dav_url) VALUES (?, ?, '#2563EB', ?) RETURNING id",
                    )
                    .bind(&account_id)
                    .bind(&email)
                    .bind(&collection)
                    .fetch_one(&user_db)
                    .await?;
                    cals.push((id, collection, None));
                }
                Err(e) => errors.push(format!("caldav discover: {e}")),
            },
            Err(e) => {
                let id: String = sqlx::query_scalar(
                    "INSERT INTO calendars (account_id, name, color, dav_url) VALUES (?, ?, '#2563EB', ?) RETURNING id",
                )
                .bind(&account_id)
                .bind(&email)
                .bind(&caldav)
                .fetch_one(&user_db)
                .await?;
                cals.push((id, caldav.clone(), None));
                errors.push(format!("caldav discover: {e}"));
            }
        }
    }
    let range_start = chrono::Utc::now() - chrono::Duration::days(365);
    let range_end = chrono::Utc::now() + chrono::Duration::days(730);
    for (cal_id, collection, stored_ctag) in cals {
        let remote_ctag = match calendar_sync::collection_ctag_with_tls_options(
            &collection,
            &cal_auth,
            trusted_cert_der.as_deref(),
            accept_invalid_tls,
        )
        .await
        {
            Ok(ctag) => ctag,
            Err(e) => {
                errors.push(format!("caldav ctag: {e}"));
                None
            }
        };
        if remote_ctag.is_some() && remote_ctag == stored_ctag {
            continue;
        }
        match calendar_sync::pull_range_with_tls_options(
            &collection,
            &cal_auth,
            Some(range_start),
            Some(range_end),
            trusted_cert_der.as_deref(),
            accept_invalid_tls,
        )
        .await
        {
            Ok(server_events) => {
                match merge_calendar(
                    &user_db,
                    &cal_id,
                    &collection,
                    &cal_auth,
                    trusted_cert_der.as_deref(),
                    accept_invalid_tls,
                    server_events,
                )
                .await
                {
                    Ok((n, mut errs)) => {
                        events_synced += n;
                        sqlx::query("UPDATE calendars SET ctag = ?, sync_token = ? WHERE id = ?")
                            .bind(&remote_ctag)
                            .bind(&remote_ctag)
                            .bind(&cal_id)
                            .execute(&user_db)
                            .await?;
                        errors.append(&mut errs);
                    }
                    Err(e) => errors.push(format!("caldav merge: {e}")),
                }
            }
            Err(e) => errors.push(format!("caldav pull: {e}")),
        }
    }

    Ok(Json(serde_json::json!({
        "contacts": contacts_synced,
        "events": events_synced,
        "errors": errors,
    })))
}

/// Discover CalDAV collection URLs for an account (RFC 6764), used by the "add
/// CalDAV calendar" flow to offer available remote calendars.
pub async fn caldav_discover(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
    Query(query): Query<CaldavDiscoverQuery>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let persist_tls_exception = query.accept_invalid_tls == Some(true)
        || query
            .tls_decision
            .is_some_and(crate::tls::TlsDecision::persists_exception);
    let retry_with_invalid_tls = query.accept_invalid_tls == Some(true)
        || query
            .tls_decision
            .is_some_and(crate::tls::TlsDecision::permits_retry);
    if persist_tls_exception {
        sqlx::query("UPDATE email_accounts SET caldav_accept_invalid_tls = 1 WHERE id = ?")
            .bind(&account_id)
            .execute(&user_db)
            .await?;
    }
    let row: Option<(String, Vec<u8>, Option<String>, Option<String>, Option<String>, bool)> = sqlx::query_as(
        "SELECT primary_email, credentials_encrypted, caldav_url, imap_tls_cert, smtp_tls_cert, caldav_accept_invalid_tls FROM email_accounts WHERE id = ?",
    )
    .bind(&account_id)
    .fetch_optional(&user_db)
    .await?;
    let (email, creds_enc, caldav_url, imap_tls_cert, smtp_tls_cert, stored_accept_invalid_tls) =
        row.ok_or(AppError::NotFound)?;

    let creds_bytes = state
        .credential_key
        .decrypt(&creds_enc)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let creds: serde_json::Value =
        serde_json::from_slice(&creds_bytes).map_err(|e| AppError::Internal(e.to_string()))?;
    let (_card, cal_auth) = build_auth(&creds, &email);
    let trusted_cert_der = mail_sync::session::decode_trusted_cert(
        imap_tls_cert.as_deref().or(smtp_tls_cert.as_deref()),
    );

    let (base, discovered) = discover_caldav_candidates(
        &email,
        caldav_url.as_deref(),
        &cal_auth,
        trusted_cert_der.as_deref(),
        retry_with_invalid_tls || stored_accept_invalid_tls,
    )
    .await
    .map_err(super::caldav_error::curated_caldav_error)?;
    let calendars: Vec<DiscoveredCalendarResponse> = discovered
        .into_iter()
        .map(|calendar| DiscoveredCalendarResponse {
            name: calendar.name,
            url: calendar.url,
            color: calendar.color,
        })
        .collect();
    let url = calendars
        .first()
        .map(|calendar| calendar.url.clone())
        .unwrap_or(base);

    Ok(Json(
        serde_json::json!({ "url": url, "calendars": calendars }),
    ))
}

/// One local calendar row, in the column order merge_calendar selects.
type LocalEvent = (
    String,         // id
    Option<String>, // uid
    Option<String>, // href
    Option<String>, // etag
    i64,            // dirty
    i64,            // deleted
    String,         // summary/title
    Option<String>, // description
    Option<String>, // location
    String,         // start_dt
    String,         // end_dt
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
    trusted_cert_der: Option<&[u8]>,
    accept_invalid_tls: bool,
    server: Vec<calendar_sync::ParsedEvent>,
) -> Result<(usize, Vec<String>), AppError> {
    use std::collections::HashSet;
    let mut errors = Vec::new();

    let local: Vec<LocalEvent> = sqlx::query_as(
        "SELECT id, uid, href, etag, dirty, deleted, summary, description, location, start_dt, end_dt, all_day \
         FROM calendar_events WHERE calendar_id = ?",
    )
    .bind(cal_id)
    .fetch_all(db)
    .await?;

    let server_uids: HashSet<&str> = server
        .iter()
        .map(|e| e.uid.as_str())
        .filter(|u| !u.is_empty())
        .collect();

    // 1. Server -> local: update non-dirty changed rows, insert new ones.
    for se in &server {
        if se.uid.is_empty() {
            continue;
        }
        match local
            .iter()
            .find(|l| l.1.as_deref() == Some(se.uid.as_str()))
        {
            Some(l) => {
                if l.4 == 0 && l.5 == 0 && l.3.as_deref() != se.etag.as_deref() {
                    sqlx::query(
                        "UPDATE calendar_events SET summary=?, description=?, location=?, start_dt=?, end_dt=?, all_day=?, href=?, etag=?, synced_at=datetime('now') WHERE id=?",
                    )
                    .bind(&se.title).bind(&se.description).bind(&se.location)
                    .bind(&se.starts_at).bind(&se.ends_at).bind(se.all_day)
                    .bind(&se.href).bind(&se.etag).bind(&l.0)
                    .execute(db).await?;
                }
            }
            None => {
                sqlx::query(
                    "INSERT INTO calendar_events (calendar_id, uid, href, etag, summary, description, location, start_dt, end_dt, all_day, dirty, deleted, synced_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, datetime('now'))",
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
                if let Err(e) = calendar_sync::delete_event_with_tls_options(
                    href,
                    auth,
                    l.3.as_deref(),
                    trusted_cert_der,
                    accept_invalid_tls,
                )
                .await
                {
                    errors.push(format!("caldav delete: {e}"));
                    continue;
                }
            }
            sqlx::query("DELETE FROM calendar_events WHERE id = ?")
                .bind(&l.0)
                .execute(db)
                .await?;
            continue;
        }

        if l.4 == 1 {
            // Dirty: push as create (no href) or update (If-Match etag).
            let uid =
                l.1.clone()
                    .unwrap_or_else(|| format!("{}@mailquill", uuid::Uuid::new_v4()));
            let resource =
                l.2.clone()
                    .unwrap_or_else(|| format!("{}/{}.ics", collection.trim_end_matches('/'), uid));
            let ics = calendar_sync::build_ics(&calendar_sync::EventInput {
                uid: &uid,
                title: &l.6,
                starts_at: &l.9,
                ends_at: &l.10,
                all_day: l.11,
                location: l.8.as_deref(),
                description: l.7.as_deref(),
                rrule: None,
                attendees_json: None,
                organizer_email: None,
                organizer_name: None,
            });
            match calendar_sync::put_event_with_tls_options(
                &resource,
                auth,
                &ics,
                l.3.as_deref(),
                trusted_cert_der,
                accept_invalid_tls,
            )
            .await
            {
                Ok(new_etag) => {
                    sqlx::query(
                        "UPDATE calendar_events SET dirty=0, uid=?, href=?, etag=? WHERE id=?",
                    )
                    .bind(&uid)
                    .bind(&resource)
                    .bind(&new_etag)
                    .bind(&l.0)
                    .execute(db)
                    .await?;
                }
                Err(e) => errors.push(format!("caldav put: {e}")),
            }
        } else if let Some(uid) = &l.1 {
            // Clean row that had a server copy but is gone from the server now —
            // the remote delete wins.
            if l.2.is_some() && !server_uids.contains(uid.as_str()) {
                sqlx::query("DELETE FROM calendar_events WHERE id = ?")
                    .bind(&l.0)
                    .execute(db)
                    .await?;
            }
        }
    }

    Ok((server.len(), errors))
}
