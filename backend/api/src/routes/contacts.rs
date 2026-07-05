use axum::{
    body::Body,
    extract::{Extension, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine;
use bytes::Bytes;
use chrono::Utc;
use contact_sync::{ContactSyncStatus, LabeledValue, ParsedContact, PostalAddress};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{error::AppError, middleware::UserId, state::AppState};

#[derive(Serialize, sqlx::FromRow)]
pub struct ContactAccount {
    id: String,
    display_name: String,
    #[serde(rename = "type")]
    account_type: String,
    base_url: Option<String>,
    auth_scheme: String,
    sync_token: Option<String>,
    last_synced_at: Option<String>,
    sync_status: String,
    sync_error: Option<String>,
}

#[derive(Deserialize)]
pub struct NewContactAccount {
    display_name: String,
    #[serde(rename = "type")]
    account_type: String,
    base_url: Option<String>,
    auth_scheme: Option<String>,
    username: Option<String>,
    password: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Serialize)]
pub struct SyncStatus {
    status: String,
    last_synced_at: Option<String>,
    error: Option<String>,
}

#[derive(Serialize)]
pub struct Contact {
    id: String,
    account_id: String,
    uid: String,
    display_name: Option<String>,
    given_name: Option<String>,
    family_name: Option<String>,
    org: Option<String>,
    title: Option<String>,
    emails: Vec<LabeledValue>,
    phones: Vec<LabeledValue>,
    addresses: Vec<PostalAddress>,
    notes: Option<String>,
    photo_blob_key: Option<String>,
    raw_vcard: Option<String>,
    synced_at: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ContactRow {
    id: String,
    account_id: String,
    uid: String,
    display_name: Option<String>,
    given_name: Option<String>,
    family_name: Option<String>,
    org: Option<String>,
    title: Option<String>,
    emails: String,
    phones: String,
    addresses: String,
    notes: Option<String>,
    photo_blob_key: Option<String>,
    raw_vcard: Option<String>,
    synced_at: Option<String>,
}

#[derive(Deserialize)]
pub struct ContactQuery {
    q: Option<String>,
    account_id: Option<String>,
}

#[derive(Deserialize)]
pub struct NewContact {
    account_id: String,
    display_name: Option<String>,
    given_name: Option<String>,
    family_name: Option<String>,
    org: Option<String>,
    title: Option<String>,
    emails: Option<Vec<LabeledValue>>,
    phones: Option<Vec<LabeledValue>>,
    addresses: Option<Vec<PostalAddress>>,
    notes: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateContact {
    display_name: Option<String>,
    given_name: Option<String>,
    family_name: Option<String>,
    org: Option<String>,
    title: Option<String>,
    emails: Option<Vec<LabeledValue>>,
    phones: Option<Vec<LabeledValue>>,
    addresses: Option<Vec<PostalAddress>>,
    notes: Option<String>,
}

const SELECT_CONTACT: &str = "id, account_id, uid, display_name, given_name, family_name, org, title, emails, phones, addresses, notes, photo_blob_key, raw_vcard, synced_at";
const CONTACT_SYNC_INTERVAL_SECS: u64 = 300;

pub async fn create_account(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<NewContactAccount>,
) -> Result<impl IntoResponse, AppError> {
    validate_account(&req)?;
    let user_db = state.user_db_pool.get(&user.0).await?;
    let auth_scheme = req.auth_scheme.clone().unwrap_or_else(|| {
        if req.access_token.is_some() {
            "oauth2".to_owned()
        } else {
            "basic".to_owned()
        }
    });
    let credentials = serde_json::json!({
        "username": req.username,
        "password": req.password,
        "access_token": req.access_token,
        "refresh_token": req.refresh_token,
    });
    let encrypted = state
        .credential_key
        .encrypt(&serde_json::to_vec(&credentials).map_err(|e| AppError::Internal(e.to_string()))?)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let id: String = sqlx::query_scalar(
        "INSERT INTO contact_accounts \
         (display_name, type, base_url, auth_scheme, credentials_encrypted, sync_status) \
         VALUES (?, ?, ?, ?, ?, 'syncing') RETURNING id",
    )
    .bind(&req.display_name)
    .bind(&req.account_type)
    .bind(&req.base_url)
    .bind(&auth_scheme)
    .bind(&encrypted)
    .fetch_one(&user_db)
    .await?;

    if let Err(err) = sync_contact_account(&user_db, &state, &id).await {
        let _ = sqlx::query("DELETE FROM contact_accounts WHERE id = ?")
            .bind(&id)
            .execute(&user_db)
            .await;
        return Err(AppError::Unprocessable(err));
    }

    spawn_contact_sync_task(state.clone(), user.0.clone(), id.clone(), false).await;

    Ok(Json(fetch_account(&user_db, &id).await?))
}

pub async fn list_accounts(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let accounts: Vec<ContactAccount> = sqlx::query_as(
        "SELECT id, display_name, type AS account_type, base_url, auth_scheme, sync_token, last_synced_at, sync_status, sync_error \
         FROM contact_accounts ORDER BY display_name COLLATE NOCASE ASC",
    )
    .fetch_all(&user_db)
    .await?;
    Ok(Json(accounts))
}

pub async fn delete_account(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    state.contact_sync_manager.stop_account(&id).await;
    let rows = sqlx::query("DELETE FROM contact_accounts WHERE id = ?")
        .bind(&id)
        .execute(&user_db)
        .await?
        .rows_affected();
    if rows == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "deleted": id })))
}

pub async fn account_sync_status(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT sync_status, last_synced_at, sync_error FROM contact_accounts WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&user_db)
    .await?;
    let Some((status, last_synced_at, error)) = row else {
        return Err(AppError::NotFound);
    };
    Ok(Json(SyncStatus {
        status,
        last_synced_at,
        error,
    }))
}

pub async fn trigger_sync(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    require_account(&user_db, &id).await?;
    sqlx::query("UPDATE contact_accounts SET sync_status = 'syncing', sync_error = NULL WHERE id = ?")
        .bind(&id)
        .execute(&user_db)
        .await?;
    spawn_contact_sync_task(state, user.0, id, true).await;
    Ok(Json(SyncStatus {
        status: "syncing".to_owned(),
        last_synced_at: None,
        error: None,
    }))
}

pub async fn list_contacts(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(query): Query<ContactQuery>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let rows: Vec<ContactRow> = match (query.q.as_deref(), query.account_id.as_deref()) {
        (Some(q), Some(account_id)) if !q.trim().is_empty() => {
            let like = format!("%{}%", q.trim());
            sqlx::query_as(&format!(
                "SELECT {SELECT_CONTACT} FROM contacts \
                 WHERE account_id = ? AND (display_name LIKE ? OR given_name LIKE ? OR family_name LIKE ? OR emails LIKE ?) \
                 ORDER BY display_name COLLATE NOCASE ASC"
            ))
            .bind(account_id)
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .fetch_all(&user_db)
            .await?
        }
        (Some(q), None) if !q.trim().is_empty() => {
            let like = format!("%{}%", q.trim());
            sqlx::query_as(&format!(
                "SELECT {SELECT_CONTACT} FROM contacts \
                 WHERE display_name LIKE ? OR given_name LIKE ? OR family_name LIKE ? OR emails LIKE ? \
                 ORDER BY display_name COLLATE NOCASE ASC"
            ))
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .bind(&like)
            .fetch_all(&user_db)
            .await?
        }
        (_, Some(account_id)) => {
            sqlx::query_as(&format!(
                "SELECT {SELECT_CONTACT} FROM contacts WHERE account_id = ? ORDER BY display_name COLLATE NOCASE ASC"
            ))
            .bind(account_id)
            .fetch_all(&user_db)
            .await?
        }
        _ => {
            sqlx::query_as(&format!(
                "SELECT {SELECT_CONTACT} FROM contacts ORDER BY display_name COLLATE NOCASE ASC"
            ))
            .fetch_all(&user_db)
            .await?
        }
    };
    Ok(Json(rows.into_iter().map(row_to_contact).collect::<Result<Vec<_>, _>>()?))
}

pub async fn search_contacts(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(query): Query<ContactQuery>,
) -> Result<impl IntoResponse, AppError> {
    let term = query.q.unwrap_or_default();
    let trimmed = term.trim();
    if trimmed.is_empty() {
        return Ok(Json(Vec::<Contact>::new()));
    }
    let user_db = state.user_db_pool.get(&user.0).await?;
    let like = format!("%{trimmed}%");
    let rows: Vec<ContactRow> = sqlx::query_as(&format!(
        "SELECT {SELECT_CONTACT} FROM contacts \
         WHERE display_name LIKE ? OR given_name LIKE ? OR family_name LIKE ? OR emails LIKE ? \
         ORDER BY CASE \
           WHEN display_name LIKE ? THEN 0 \
           WHEN given_name LIKE ? THEN 1 \
           WHEN family_name LIKE ? THEN 2 \
           ELSE 3 END, display_name COLLATE NOCASE ASC \
         LIMIT 10"
    ))
    .bind(&like)
    .bind(&like)
    .bind(&like)
    .bind(&like)
    .bind(format!("{trimmed}%"))
    .bind(format!("{trimmed}%"))
    .bind(format!("{trimmed}%"))
    .fetch_all(&user_db)
    .await?;
    Ok(Json(rows.into_iter().map(row_to_contact).collect::<Result<Vec<_>, _>>()?))
}

pub async fn create_contact(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<NewContact>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    require_account(&user_db, &req.account_id).await?;
    if req.display_name.as_deref().unwrap_or("").trim().is_empty()
        && req.given_name.as_deref().unwrap_or("").trim().is_empty()
        && req.family_name.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err(AppError::Unprocessable(
            "display_name or name parts are required".into(),
        ));
    }
    let mut contact = ParsedContact {
        uid: Uuid::new_v4().to_string(),
        display_name: req.display_name,
        given_name: req.given_name,
        family_name: req.family_name,
        org: req.org,
        title: req.title,
        emails: req.emails.unwrap_or_default(),
        phones: req.phones.unwrap_or_default(),
        addresses: req.addresses.unwrap_or_default(),
        notes: req.notes,
        photo_reference: None,
        raw_vcard: None,
    };
    write_remote(&user_db, &state, &req.account_id, None, &mut contact).await?;
    let id = upsert_contact(&user_db, &req.account_id, contact).await?;
    Ok(Json(fetch_contact(&user_db, &id).await?))
}

pub async fn update_contact(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
    Json(req): Json<UpdateContact>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let existing = fetch_contact(&user_db, &id).await?;
    let mut contact = ParsedContact {
        uid: existing.uid.clone(),
        display_name: req.display_name.or(existing.display_name),
        given_name: req.given_name.or(existing.given_name),
        family_name: req.family_name.or(existing.family_name),
        org: req.org.or(existing.org),
        title: req.title.or(existing.title),
        emails: req.emails.unwrap_or(existing.emails),
        phones: req.phones.unwrap_or(existing.phones),
        addresses: req.addresses.unwrap_or(existing.addresses),
        notes: req.notes.or(existing.notes),
        photo_reference: None,
        raw_vcard: existing.raw_vcard,
    };
    write_remote(&user_db, &state, &existing.account_id, Some(&existing.uid), &mut contact).await?;
    upsert_contact(&user_db, &existing.account_id, contact).await?;
    Ok(Json(fetch_contact(&user_db, &id).await?))
}

pub async fn delete_contact(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let contact = fetch_contact(&user_db, &id).await?;
    delete_remote(&user_db, &state, &contact.account_id, &contact.uid).await?;
    let rows = sqlx::query("DELETE FROM contacts WHERE id = ?")
        .bind(&id)
        .execute(&user_db)
        .await?
        .rows_affected();
    if rows == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "deleted": id })))
}

pub async fn get_photo(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let contact = fetch_contact(&user_db, &id).await?;
    if let Some(key) = &contact.photo_blob_key {
        let bytes = state
            .blob_store
            .get(key)
            .await
            .map_err(|_| AppError::NotFound)?;
        return Ok(binary_response(bytes, "application/octet-stream"));
    }
    let Some(raw_vcard) = &contact.raw_vcard else {
        return Err(AppError::NotFound);
    };
    let Some((bytes, content_type)) = photo_from_vcard(raw_vcard).await? else {
        return Err(AppError::NotFound);
    };
    let key = format!("contact/{}/{}/photo", contact.account_id, contact.uid);
    state
        .blob_store
        .put(&key, bytes.clone())
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    sqlx::query("UPDATE contacts SET photo_blob_key = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(&key)
        .bind(&id)
        .execute(&user_db)
        .await?;
    Ok(binary_response(bytes, &content_type))
}

async fn sync_contact_account(
    db: &sqlx::SqlitePool,
    state: &AppState,
    account_id: &str,
) -> Result<(), String> {
    sqlx::query("UPDATE contact_accounts SET sync_status = 'syncing', sync_error = NULL WHERE id = ?")
        .bind(account_id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    let result = sync_contact_account_inner(db, state, account_id).await;
    match &result {
        Ok(()) => {
            let _ = sqlx::query(
                "UPDATE contact_accounts SET sync_status='idle', last_synced_at=?, sync_error=NULL WHERE id=?",
            )
            .bind(Utc::now().to_rfc3339())
            .bind(account_id)
            .execute(db)
            .await;
        }
        Err(err) => {
            let _ = sqlx::query(
                "UPDATE contact_accounts SET sync_status='error', sync_error=? WHERE id=?",
            )
            .bind(err)
            .bind(account_id)
            .execute(db)
            .await;
        }
    }
    result
}

pub async fn spawn_contact_sync_task(
    state: AppState,
    user_id: String,
    account_id: String,
    run_immediately: bool,
) {
    let manager = state.contact_sync_manager.clone();
    let manager_for_task = manager.clone();
    let account_for_task = account_id.clone();
    manager
        .start_account(account_id, async move {
            let mut should_run = run_immediately;
            loop {
                if !should_run {
                    tokio::time::sleep(std::time::Duration::from_secs(CONTACT_SYNC_INTERVAL_SECS)).await;
                }
                should_run = false;
                let status = match state.user_db_pool.get(&user_id).await {
                    Ok(db) => match sync_contact_account(&db, &state, &account_for_task).await {
                        Ok(()) => ContactSyncStatus {
                            state: "idle".to_owned(),
                            last_synced_at: Some(Utc::now().to_rfc3339()),
                            error: None,
                        },
                        Err(err) => ContactSyncStatus {
                            state: "error".to_owned(),
                            last_synced_at: None,
                            error: Some(err),
                        },
                    },
                    Err(err) => ContactSyncStatus {
                        state: "error".to_owned(),
                        last_synced_at: None,
                        error: Some(err.to_string()),
                    },
                };
                manager_for_task
                    .update_status(&account_for_task, status)
                    .await;
            }
        })
        .await;
}

async fn sync_contact_account_inner(
    db: &sqlx::SqlitePool,
    state: &AppState,
    account_id: &str,
) -> Result<(), String> {
    let (provider, base_url, auth_scheme, encrypted, sync_token): (
        String,
        Option<String>,
        String,
        Vec<u8>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT type, base_url, auth_scheme, credentials_encrypted, sync_token FROM contact_accounts WHERE id = ?",
    )
    .bind(account_id)
    .fetch_one(db)
    .await
    .map_err(|e| e.to_string())?;
    let creds = decrypt_credentials(state, &encrypted)?;
    let mut next_token = sync_token;
    let contacts = match provider.as_str() {
        "cardav" => {
            let base = base_url.ok_or_else(|| "base_url is required".to_string())?;
            let auth = dav_auth(&auth_scheme, &creds)?;
            let discovered = contact_sync::discover_carddav_addressbook(&base, &auth).await?;
            sqlx::query("UPDATE contact_accounts SET base_url = ? WHERE id = ?")
                .bind(&discovered)
                .bind(account_id)
                .execute(db)
                .await
                .map_err(|e| e.to_string())?;
            next_token = contact_sync::carddav_sync_token(&discovered, &auth).await?;
            contact_sync::sync_carddav(&discovered, &auth).await?
        }
        "graph" => {
            let token = creds["access_token"]
                .as_str()
                .ok_or_else(|| "access_token is required".to_string())?;
            let page = contact_sync::graph_contacts(token, next_token.as_deref()).await?;
            next_token = page.delta_link.or(page.next_link);
            page.contacts
        }
        "google" => {
            let token = creds["access_token"]
                .as_str()
                .ok_or_else(|| "access_token is required".to_string())?;
            let page = contact_sync::google_connections(token, next_token.as_deref()).await?;
            next_token = page.sync_token.or(page.next_page_token);
            page.contacts
        }
        _ => return Err("unsupported contact provider".to_string()),
    };

    sqlx::query("DELETE FROM contacts WHERE account_id = ?")
        .bind(account_id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    for contact in contacts {
        upsert_contact(db, account_id, contact)
            .await
            .map_err(|e| e.to_string())?;
    }
    sqlx::query("UPDATE contact_accounts SET sync_token = ? WHERE id = ?")
        .bind(next_token)
        .bind(account_id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn upsert_contact(
    db: &sqlx::SqlitePool,
    account_id: &str,
    mut contact: ParsedContact,
) -> Result<String, AppError> {
    if contact.uid.trim().is_empty() {
        contact.uid = Uuid::new_v4().to_string();
    }
    if contact.raw_vcard.is_none() {
        contact.raw_vcard = Some(contact_sync::contact_to_vcard(&contact));
    }
    let emails = serde_json::to_string(&contact.emails).map_err(|e| AppError::Internal(e.to_string()))?;
    let phones = serde_json::to_string(&contact.phones).map_err(|e| AppError::Internal(e.to_string()))?;
    let addresses =
        serde_json::to_string(&contact.addresses).map_err(|e| AppError::Internal(e.to_string()))?;
    let id: String = sqlx::query_scalar(
        "INSERT INTO contacts \
         (account_id, uid, display_name, given_name, family_name, org, title, emails, phones, addresses, notes, raw_vcard, synced_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(account_id, uid) DO UPDATE SET \
           display_name=excluded.display_name, given_name=excluded.given_name, family_name=excluded.family_name, \
           org=excluded.org, title=excluded.title, emails=excluded.emails, phones=excluded.phones, \
           addresses=excluded.addresses, notes=excluded.notes, raw_vcard=excluded.raw_vcard, synced_at=excluded.synced_at, \
           updated_at=datetime('now') \
         RETURNING id",
    )
    .bind(account_id)
    .bind(&contact.uid)
    .bind(&contact.display_name)
    .bind(&contact.given_name)
    .bind(&contact.family_name)
    .bind(&contact.org)
    .bind(&contact.title)
    .bind(&emails)
    .bind(&phones)
    .bind(&addresses)
    .bind(&contact.notes)
    .bind(&contact.raw_vcard)
    .bind(Utc::now().to_rfc3339())
    .fetch_one(db)
    .await?;
    Ok(id)
}

async fn write_remote(
    db: &sqlx::SqlitePool,
    state: &AppState,
    account_id: &str,
    existing_uid: Option<&str>,
    contact: &mut ParsedContact,
) -> Result<(), AppError> {
    let (provider, base_url, auth_scheme, encrypted): (String, Option<String>, String, Vec<u8>) =
        sqlx::query_as(
            "SELECT type, base_url, auth_scheme, credentials_encrypted FROM contact_accounts WHERE id = ?",
        )
        .bind(account_id)
        .fetch_one(db)
        .await?;
    let creds = decrypt_credentials(state, &encrypted).map_err(AppError::BadGateway)?;
    match provider.as_str() {
        "cardav" => {
            let base = base_url.ok_or_else(|| AppError::Unprocessable("base_url is required".into()))?;
            let auth = dav_auth(&auth_scheme, &creds).map_err(AppError::BadGateway)?;
            let raw = contact_sync::contact_to_vcard(contact);
            let href = carddav_href(&base, existing_uid.unwrap_or(&contact.uid));
            contact_sync::put_carddav_contact(&href, &auth, &raw)
                .await
                .map_err(AppError::BadGateway)?;
            contact.raw_vcard = Some(raw);
        }
        "graph" => {
            let token = creds["access_token"]
                .as_str()
                .ok_or_else(|| AppError::BadGateway("access_token is required".into()))?;
            if let Some(uid) = existing_uid {
                contact_sync::graph_update_contact(token, uid, contact)
                    .await
                    .map_err(AppError::BadGateway)?;
            } else {
                contact.uid = contact_sync::graph_create_contact(token, contact)
                    .await
                    .map_err(AppError::BadGateway)?;
            }
        }
        "google" => {
            let token = creds["access_token"]
                .as_str()
                .ok_or_else(|| AppError::BadGateway("access_token is required".into()))?;
            if let Some(uid) = existing_uid {
                contact_sync::google_update_contact(token, uid, contact)
                    .await
                    .map_err(AppError::BadGateway)?;
            } else {
                contact.uid = contact_sync::google_create_contact(token, contact)
                    .await
                    .map_err(AppError::BadGateway)?;
            }
        }
        _ => return Err(AppError::Unprocessable("unsupported contact provider".into())),
    }
    Ok(())
}

async fn delete_remote(
    db: &sqlx::SqlitePool,
    state: &AppState,
    account_id: &str,
    uid: &str,
) -> Result<(), AppError> {
    let (provider, base_url, auth_scheme, encrypted): (String, Option<String>, String, Vec<u8>) =
        sqlx::query_as(
            "SELECT type, base_url, auth_scheme, credentials_encrypted FROM contact_accounts WHERE id = ?",
        )
        .bind(account_id)
        .fetch_one(db)
        .await?;
    let creds = decrypt_credentials(state, &encrypted).map_err(AppError::BadGateway)?;
    match provider.as_str() {
        "cardav" => {
            let base = base_url.ok_or_else(|| AppError::Unprocessable("base_url is required".into()))?;
            let auth = dav_auth(&auth_scheme, &creds).map_err(AppError::BadGateway)?;
            contact_sync::delete_carddav_contact(&carddav_href(&base, uid), &auth)
                .await
                .map_err(AppError::BadGateway)?;
        }
        "graph" => {
            let token = creds["access_token"]
                .as_str()
                .ok_or_else(|| AppError::BadGateway("access_token is required".into()))?;
            contact_sync::graph_delete_contact(token, uid)
                .await
                .map_err(AppError::BadGateway)?;
        }
        "google" => {
            let token = creds["access_token"]
                .as_str()
                .ok_or_else(|| AppError::BadGateway("access_token is required".into()))?;
            contact_sync::google_delete_contact(token, uid)
                .await
                .map_err(AppError::BadGateway)?;
        }
        _ => return Err(AppError::Unprocessable("unsupported contact provider".into())),
    }
    Ok(())
}

async fn fetch_account(db: &sqlx::SqlitePool, id: &str) -> Result<ContactAccount, AppError> {
    sqlx::query_as(
        "SELECT id, display_name, type AS account_type, base_url, auth_scheme, sync_token, last_synced_at, sync_status, sync_error \
         FROM contact_accounts WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound)
}

async fn require_account(db: &sqlx::SqlitePool, id: &str) -> Result<(), AppError> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM contact_accounts WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?;
    exists.map(|_| ()).ok_or(AppError::NotFound)
}

async fn fetch_contact(db: &sqlx::SqlitePool, id: &str) -> Result<Contact, AppError> {
    let row: ContactRow = sqlx::query_as(&format!("SELECT {SELECT_CONTACT} FROM contacts WHERE id = ?"))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or(AppError::NotFound)?;
    row_to_contact(row)
}

fn row_to_contact(row: ContactRow) -> Result<Contact, AppError> {
    Ok(Contact {
        id: row.id,
        account_id: row.account_id,
        uid: row.uid,
        display_name: row.display_name,
        given_name: row.given_name,
        family_name: row.family_name,
        org: row.org,
        title: row.title,
        emails: serde_json::from_str(&row.emails).map_err(|e| AppError::Internal(e.to_string()))?,
        phones: serde_json::from_str(&row.phones).map_err(|e| AppError::Internal(e.to_string()))?,
        addresses: serde_json::from_str(&row.addresses)
            .map_err(|e| AppError::Internal(e.to_string()))?,
        notes: row.notes,
        photo_blob_key: row.photo_blob_key,
        raw_vcard: row.raw_vcard,
        synced_at: row.synced_at,
    })
}

fn validate_account(req: &NewContactAccount) -> Result<(), AppError> {
    match req.account_type.as_str() {
        "cardav" if req.base_url.is_none() => {
            Err(AppError::Unprocessable("base_url is required".into()))
        }
        "cardav" if req.password.is_none() && req.access_token.is_none() => {
            Err(AppError::Unprocessable("credentials are required".into()))
        }
        "graph" | "google" if req.access_token.is_none() => {
            Err(AppError::Unprocessable("access_token is required".into()))
        }
        "cardav" | "graph" | "google" => Ok(()),
        _ => Err(AppError::Unprocessable(
            "unsupported contact provider".into(),
        )),
    }
}

fn decrypt_credentials(state: &AppState, encrypted: &[u8]) -> Result<Value, String> {
    let bytes = state
        .credential_key
        .decrypt(encrypted)
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn dav_auth(auth_scheme: &str, creds: &Value) -> Result<contact_sync::DavAuth, String> {
    if auth_scheme == "oauth2" {
        return Ok(contact_sync::DavAuth::Bearer(
            creds["access_token"].as_str().unwrap_or_default().to_owned(),
        ));
    }
    Ok(contact_sync::DavAuth::Basic {
        username: creds["username"].as_str().unwrap_or_default().to_owned(),
        password: creds["password"].as_str().unwrap_or_default().to_owned(),
    })
}

fn carddav_href(base: &str, uid: &str) -> String {
    let safe_uid = uid.replace(['/', '\\', '?', '#'], "-");
    format!("{}/{}.vcf", base.trim_end_matches('/'), safe_uid)
}

async fn photo_from_vcard(raw_vcard: &str) -> Result<Option<(Bytes, String)>, AppError> {
    for line in raw_vcard.replace("\r\n", "\n").lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if !key
            .split(';')
            .next()
            .unwrap_or_default()
            .eq_ignore_ascii_case("PHOTO")
        {
            continue;
        }
        if value.starts_with("http://") || value.starts_with("https://") {
            let bytes = reqwest::get(value)
                .await
                .map_err(|e| AppError::BadGateway(e.to_string()))?
                .bytes()
                .await
                .map_err(|e| AppError::BadGateway(e.to_string()))?;
            return Ok(Some((bytes, "application/octet-stream".to_owned())));
        }
        let cleaned = value.trim();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(cleaned)
            .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(cleaned))
            .map_err(|e| AppError::Unprocessable(e.to_string()))?;
        let content_type = key
            .split(';')
            .find_map(|part| part.strip_prefix("MEDIATYPE=").or_else(|| part.strip_prefix("TYPE=")))
            .map(str::to_owned)
            .unwrap_or_else(|| "image/jpeg".to_owned());
        return Ok(Some((Bytes::from(bytes), content_type)));
    }
    Ok(None)
}

fn binary_response(bytes: Bytes, content_type: &str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type.to_owned()),
            (header::CACHE_CONTROL, "private, max-age=86400".to_owned()),
        ],
        Body::from(bytes),
    )
        .into_response()
}
