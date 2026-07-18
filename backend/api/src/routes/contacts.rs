use std::collections::HashSet;

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
use contact_sync::{
    adapters::{
        CardDavAdapter, ContactProviderAdapter, GooglePeopleAdapter, MicrosoftGraphAdapter,
    },
    orchestration::{
        create_remote_first, delete_remote_first, provider_error_code, sync_source_once,
        update_remote_first, RetryPolicy,
    },
    ContactSyncStatus, LabeledValue, ParsedContact, PostalAddress, ProviderError,
    ProviderErrorCategory, CARDDAV_TLS_VERIFICATION_FAILED,
};
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
    email_account_id: Option<String>,
    management_mode: String,
    capability_state: String,
    capability_reason: Option<String>,
    enabled: bool,
    cache_retained: bool,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ContactBookResponse {
    id: String,
    account_id: String,
    remote_id: String,
    display_name: String,
    parent_remote_id: Option<String>,
    is_default: bool,
    is_writable: bool,
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

#[derive(Deserialize)]
pub struct DisableContactsRequest {
    #[serde(default = "default_true")]
    keep_downloaded_contacts: bool,
}

#[derive(Default, Deserialize)]
pub struct DiscoverContactsRequest {
    selected_book_remote_ids: Option<Vec<String>>,
    #[serde(default)]
    accept_invalid_tls: bool,
    tls_decision: Option<crate::tls::TlsDecision>,
}

fn default_true() -> bool {
    true
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
    book_id: Option<String>,
    remote_version: Option<String>,
    photo_reference: Option<String>,
    photo_version: Option<String>,
    photo_content_type: Option<String>,
    source_email_account_id: Option<String>,
    source_provider: String,
    source_state: String,
    source_enabled: bool,
    source_writable: bool,
    groups: Vec<ContactGroup>,
}

#[derive(Serialize)]
pub struct ContactGroup {
    id: String,
    name: String,
    remote_id: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ContactGroupSummary {
    id: String,
    account_id: String,
    book_id: Option<String>,
    name: String,
    remote_id: Option<String>,
    member_count: i64,
}

#[derive(Serialize)]
pub struct ContactPage {
    items: Vec<Contact>,
    total: i64,
    limit: i64,
    offset: i64,
}

#[derive(Serialize)]
pub struct RecipientSuggestion {
    id: String,
    display_name: Option<String>,
    email: String,
    source: &'static str,
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
    book_id: Option<String>,
    remote_version: Option<String>,
    photo_reference: Option<String>,
    photo_version: Option<String>,
    photo_content_type: Option<String>,
}

#[derive(Deserialize)]
pub struct ContactQuery {
    q: Option<String>,
    account_id: Option<String>,
    mailbox_id: Option<String>,
    book_id: Option<String>,
    group_id: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct ContactGroupQuery {
    account_id: Option<String>,
    mailbox_id: Option<String>,
    book_id: Option<String>,
}

#[derive(Deserialize)]
pub struct NewContact {
    account_id: String,
    book_id: Option<String>,
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

const SELECT_CONTACT: &str = "id, account_id, uid, display_name, given_name, family_name, org, title, emails, phones, addresses, notes, photo_blob_key, raw_vcard, synced_at, book_id, remote_version, photo_reference, photo_version, photo_content_type";
const SELECT_CONTACT_QUALIFIED: &str = "c.id, c.account_id, c.uid, c.display_name, c.given_name, c.family_name, c.org, c.title, c.emails, c.phones, c.addresses, c.notes, c.photo_blob_key, c.raw_vcard, c.synced_at, c.book_id, c.remote_version, c.photo_reference, c.photo_version, c.photo_content_type";
const CONTACT_SYNC_INTERVAL_SECS: u64 = 300;

pub async fn create_account(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<NewContactAccount>,
) -> Result<impl IntoResponse, AppError> {
    validate_account(&req)?;
    if req.account_type != "cardav" {
        return Err(AppError::Unprocessable(
            "Google and Microsoft contacts are enabled from their mailbox settings".into(),
        ));
    }
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
        return Err(AppError::Unprocessable(err.to_string()));
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
        "SELECT id, display_name, type AS account_type, base_url, auth_scheme, sync_token, last_synced_at, sync_status, sync_error, \
                email_account_id, management_mode, capability_state, capability_reason, enabled, cache_retained \
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
    let management_mode: Option<String> =
        sqlx::query_scalar("SELECT management_mode FROM contact_accounts WHERE id = ?")
            .bind(&id)
            .fetch_optional(&user_db)
            .await?;
    match management_mode.as_deref() {
        None => return Err(AppError::NotFound),
        Some("mailbox") => {
            return Err(AppError::Conflict(
                "mailbox-managed contact sources must be disabled from mailbox settings".into(),
            ))
        }
        Some(_) => {}
    }
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
        "SELECT capability_state, last_synced_at, capability_reason FROM contact_accounts WHERE id = ?",
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

pub async fn list_contact_books(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    require_account(&user_db, &id).await?;
    let books: Vec<ContactBookResponse> = sqlx::query_as(
        "SELECT id, account_id, remote_id, display_name, parent_remote_id, is_default, is_writable
         FROM contact_books WHERE account_id = ?
         ORDER BY is_default DESC, display_name COLLATE NOCASE",
    )
    .bind(&id)
    .fetch_all(&user_db)
    .await?;
    Ok(Json(books))
}

pub async fn trigger_sync(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    require_account(&user_db, &id).await?;
    sqlx::query(
        "UPDATE contact_accounts SET sync_status = 'syncing', sync_error = NULL WHERE id = ?",
    )
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

pub async fn enable_mailbox_contacts(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let capability = crate::contact_reconcile::reconcile_mailbox_contact_source(
        &user_db,
        &state.credential_key,
        &account_id,
        Some(true),
    )
    .await
    .map_err(AppError::Unprocessable)?;
    if capability.state == "pending" {
        spawn_contact_sync_task(state.clone(), user.0, capability.source_id.clone(), true).await;
    }
    Ok(Json(fetch_account(&user_db, &capability.source_id).await?))
}

pub async fn disable_mailbox_contacts(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
    Json(request): Json<DisableContactsRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let source_id: String = sqlx::query_scalar(
        "SELECT id FROM contact_accounts WHERE email_account_id = ? AND management_mode = 'mailbox'",
    )
    .bind(&account_id)
    .fetch_optional(&user_db)
    .await?
    .ok_or(AppError::NotFound)?;
    state.contact_sync_manager.stop_account(&source_id).await;
    let mut tx = user_db.begin().await?;
    sqlx::query(
        "UPDATE contact_accounts
         SET enabled = 0, capability_state = 'disabled', capability_reason = ?,
             cache_retained = ?, sync_status = 'idle', sync_error = NULL
         WHERE id = ?",
    )
    .bind(if request.keep_downloaded_contacts {
        "cache_retained"
    } else {
        "cache_removed"
    })
    .bind(request.keep_downloaded_contacts)
    .bind(&source_id)
    .execute(&mut *tx)
    .await?;
    if !request.keep_downloaded_contacts {
        sqlx::query("DELETE FROM contacts WHERE account_id = ?")
            .bind(&source_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM contact_groups WHERE account_id = ?")
            .bind(&source_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM contact_books WHERE account_id = ?")
            .bind(&source_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(fetch_account(&user_db, &source_id).await?))
}

pub async fn discover_mailbox_contacts(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
    Json(request): Json<DiscoverContactsRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let source_id: String =
        sqlx::query_scalar("SELECT id FROM contact_accounts WHERE email_account_id = ?")
            .bind(&account_id)
            .fetch_optional(&user_db)
            .await?
            .ok_or(AppError::NotFound)?;
    let persist_tls_exception = request.accept_invalid_tls
        || request
            .tls_decision
            .is_some_and(crate::tls::TlsDecision::persists_exception);
    let retry_with_invalid_tls = request.accept_invalid_tls
        || request
            .tls_decision
            .is_some_and(crate::tls::TlsDecision::permits_retry);
    if persist_tls_exception {
        enable_mailbox_dav_tls_exception(&user_db, &account_id, &source_id).await?;
    }
    let adapter = contact_adapter_with_tls_override(
        &user_db,
        &state,
        &source_id,
        retry_with_invalid_tls.then_some(true),
    )
    .await
    .map_err(provider_discovery_app_error)?;
    let books = adapter
        .books()
        .await
        .map_err(provider_discovery_app_error)?;
    if let Some(selected) = request.selected_book_remote_ids {
        if selected.is_empty()
            || selected
                .iter()
                .any(|id| !books.iter().any(|book| &book.remote_id == id))
        {
            return Err(AppError::Unprocessable(
                "select at least one discovered address book".into(),
            ));
        }
        let metadata: String =
            sqlx::query_scalar("SELECT provider_metadata FROM contact_accounts WHERE id = ?")
                .bind(&source_id)
                .fetch_one(&user_db)
                .await?;
        let mut metadata =
            serde_json::from_str::<Value>(&metadata).unwrap_or_else(|_| serde_json::json!({}));
        metadata["selected_book_remote_ids"] = serde_json::json!(selected);
        sqlx::query(
            "UPDATE contact_accounts SET provider_metadata = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(metadata.to_string())
        .bind(&source_id)
        .execute(&user_db)
        .await?;
    }
    Ok(Json(serde_json::json!({
        "source_id": source_id,
        "books": books,
    })))
}

async fn enable_mailbox_dav_tls_exception(
    db: &sqlx::SqlitePool,
    email_account_id: &str,
    source_id: &str,
) -> Result<(), AppError> {
    let updated = sqlx::query(
        "UPDATE email_accounts
         SET caldav_accept_invalid_tls = 1
         WHERE id = ?
           AND EXISTS (
               SELECT 1 FROM contact_accounts
               WHERE id = ? AND email_account_id = email_accounts.id
                 AND management_mode = 'mailbox' AND type = 'cardav'
           )",
    )
    .bind(email_account_id)
    .bind(source_id)
    .execute(db)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(AppError::Unprocessable(
            "TLS exceptions are only available for mailbox CardDAV sources".to_owned(),
        ));
    }
    Ok(())
}

pub async fn list_contacts(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(query): Query<ContactQuery>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    Ok(Json(contact_page(&user_db, &query, false).await?))
}

pub async fn list_contact_groups(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(query): Query<ContactGroupQuery>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    Ok(Json(contact_groups(&user_db, &query).await?))
}

async fn contact_groups(
    db: &sqlx::SqlitePool,
    query: &ContactGroupQuery,
) -> Result<Vec<ContactGroupSummary>, AppError> {
    let groups = sqlx::query_as(
        "SELECT g.id, g.account_id, g.book_id, g.name, g.remote_id,
                count(DISTINCT gm.contact_id) AS member_count
         FROM contact_groups AS g
         JOIN contact_accounts AS ca ON ca.id = g.account_id
         LEFT JOIN contact_group_members AS gm ON gm.group_id = g.id
         WHERE (? IS NULL OR g.account_id = ?)
           AND (? IS NULL OR ca.email_account_id = ?)
           AND (? IS NULL OR g.book_id = ?)
         GROUP BY g.id, g.account_id, g.book_id, g.name, g.remote_id
         ORDER BY g.name COLLATE NOCASE, g.id",
    )
    .bind(query.account_id.as_deref())
    .bind(query.account_id.as_deref())
    .bind(query.mailbox_id.as_deref())
    .bind(query.mailbox_id.as_deref())
    .bind(query.book_id.as_deref())
    .bind(query.book_id.as_deref())
    .fetch_all(db)
    .await?;
    Ok(groups)
}

pub async fn search_contacts(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(query): Query<ContactQuery>,
) -> Result<impl IntoResponse, AppError> {
    let term = query.q.clone().unwrap_or_default();
    let trimmed = term.trim();
    if trimmed.is_empty() {
        return Ok(Json(Vec::<Contact>::new()));
    }
    let user_db = state.user_db_pool.get(&user.0).await?;
    let mut autocomplete_query = query;
    autocomplete_query.limit = Some(10);
    autocomplete_query.offset = Some(0);
    Ok(Json(
        contact_page(&user_db, &autocomplete_query, true)
            .await?
            .items,
    ))
}

pub async fn recipient_suggestions(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(query): Query<ContactQuery>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    Ok(Json(recipient_suggestion_rows(&user_db, &query).await?))
}

async fn recipient_suggestion_rows(
    db: &sqlx::SqlitePool,
    query: &ContactQuery,
) -> Result<Vec<RecipientSuggestion>, AppError> {
    let term = query.q.as_deref().map(str::trim).unwrap_or_default();
    if term.is_empty() {
        return Ok(Vec::new());
    }
    let contact_query = ContactQuery {
        q: Some(term.to_owned()),
        account_id: None,
        mailbox_id: None,
        book_id: None,
        group_id: None,
        limit: Some(10),
        offset: Some(0),
    };
    let mut contacts = contact_page(db, &contact_query, true).await?.items;
    contacts.sort_by_key(|contact| {
        contact.source_email_account_id.as_deref() != query.mailbox_id.as_deref()
    });

    let mut seen = HashSet::new();
    let mut suggestions = Vec::new();
    for contact in contacts {
        for email in contact.emails.into_iter().take(2) {
            let normalized = email.value.trim().to_ascii_lowercase();
            if valid_sender_email(&normalized) && seen.insert(normalized.clone()) {
                suggestions.push(RecipientSuggestion {
                    id: format!("contact:{}:{normalized}", contact.id),
                    display_name: contact.display_name.clone(),
                    email: email.value,
                    source: "contact",
                });
            }
        }
    }

    let own_addresses: Vec<String> = sqlx::query_scalar(
        "SELECT lower(primary_email) FROM email_accounts
         UNION SELECT lower(email) FROM account_aliases",
    )
    .fetch_all(db)
    .await?;
    let own_addresses = own_addresses.into_iter().collect::<HashSet<_>>();
    let like = format!("%{term}%");
    let senders: Vec<String> = sqlx::query_scalar(
        "SELECT from_addr
         FROM messages
         WHERE is_deleted = 0 AND from_addr <> '' AND from_addr LIKE ?
         GROUP BY lower(from_addr)
         ORDER BY max(CASE WHEN account_id = ? THEN 1 ELSE 0 END) DESC,
                  count(*) DESC, max(internal_date) DESC
         LIMIT 30",
    )
    .bind(&like)
    .bind(query.mailbox_id.as_deref())
    .fetch_all(db)
    .await?;
    for sender in senders {
        let Some((display_name, email)) = parse_sender_identity(&sender) else {
            continue;
        };
        let normalized = email.to_ascii_lowercase();
        if own_addresses.contains(&normalized) || !seen.insert(normalized.clone()) {
            continue;
        }
        suggestions.push(RecipientSuggestion {
            id: format!("sender:{normalized}"),
            display_name,
            email,
            source: "sender",
        });
        if suggestions.len() >= 12 {
            break;
        }
    }
    suggestions.truncate(12);
    Ok(suggestions)
}

fn parse_sender_identity(raw: &str) -> Option<(Option<String>, String)> {
    let raw = raw.trim();
    let (name, email) = if raw.ends_with('>') {
        let start = raw.rfind('<')?;
        (
            raw[..start].trim().trim_matches('"'),
            raw[start + 1..raw.len() - 1].trim(),
        )
    } else {
        ("", raw)
    };
    valid_sender_email(email).then(|| {
        (
            (!name.is_empty()).then(|| name.to_owned()),
            email.to_owned(),
        )
    })
}

fn valid_sender_email(email: &str) -> bool {
    let mut parts = email.split('@');
    parts.next().is_some_and(|part| !part.is_empty())
        && parts.next().is_some_and(|part| !part.is_empty())
        && parts.next().is_none()
        && !email.chars().any(char::is_whitespace)
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
    let contact = ParsedContact {
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
    let book = writable_book(&user_db, &req.account_id, req.book_id.as_deref()).await?;
    let adapter = contact_adapter(&user_db, &state, &req.account_id)
        .await
        .map_err(provider_app_error)?;
    let mutation =
        create_remote_first(&user_db, &req.account_id, adapter, &book.identity, &contact)
            .await
            .map_err(provider_app_error)?;
    Ok(Json(
        fetch_contact_by_remote_id(&user_db, &req.account_id, &mutation.remote_id).await?,
    ))
}

pub async fn update_contact(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
    Json(req): Json<UpdateContact>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let existing = fetch_contact(&user_db, &id).await?;
    let contact = ParsedContact {
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
    let book = writable_book(&user_db, &existing.account_id, existing.book_id.as_deref()).await?;
    let adapter = contact_adapter(&user_db, &state, &existing.account_id)
        .await
        .map_err(provider_app_error)?;
    update_remote_first(
        &user_db,
        &existing.account_id,
        adapter,
        &book.identity,
        &existing.uid,
        existing.remote_version.as_deref(),
        &contact,
    )
    .await
    .map_err(provider_app_error)?;
    Ok(Json(fetch_contact(&user_db, &id).await?))
}

pub async fn delete_contact(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let contact = fetch_contact(&user_db, &id).await?;
    let book = writable_book(&user_db, &contact.account_id, contact.book_id.as_deref()).await?;
    let adapter = contact_adapter(&user_db, &state, &contact.account_id)
        .await
        .map_err(provider_app_error)?;
    delete_remote_first(
        &user_db,
        &contact.account_id,
        adapter,
        &book.identity,
        &contact.uid,
        contact.remote_version.as_deref(),
    )
    .await
    .map_err(provider_app_error)?;
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
        return Ok(binary_response(
            bytes,
            contact
                .photo_content_type
                .as_deref()
                .unwrap_or("application/octet-stream"),
        ));
    }
    if let Some(book_id) = contact.book_id.as_deref() {
        let book = contact_book(&user_db, &contact.account_id, book_id).await?;
        let adapter = contact_adapter(&user_db, &state, &contact.account_id)
            .await
            .map_err(provider_app_error)?;
        if let Some(photo) = adapter
            .photo(
                &book.identity,
                &contact.uid,
                contact.photo_reference.as_deref(),
            )
            .await
            .map_err(provider_app_error)?
        {
            let key = format!("contact/{}/{}/photo", contact.account_id, contact.uid);
            let bytes = Bytes::from(photo.bytes);
            state
                .blob_store
                .put(&key, bytes.clone())
                .await
                .map_err(|error| AppError::Internal(error.to_string()))?;
            sqlx::query(
                "UPDATE contacts
                 SET photo_blob_key = ?, photo_content_type = ?, photo_version = COALESCE(?, photo_version),
                     updated_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(&key)
            .bind(&photo.content_type)
            .bind(&photo.version)
            .bind(&id)
            .execute(&user_db)
            .await?;
            return Ok(binary_response(bytes, &photo.content_type));
        }
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
    sqlx::query(
        "UPDATE contacts SET photo_blob_key = ?, updated_at = datetime('now') WHERE id = ?",
    )
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
) -> Result<(), ProviderError> {
    sync_contact_account_inner(db, state, account_id).await
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
            let mut consecutive_failures = 0u32;
            let retry_policy = RetryPolicy::default();
            loop {
                if !should_run {
                    tokio::time::sleep(std::time::Duration::from_secs(CONTACT_SYNC_INTERVAL_SECS))
                        .await;
                }
                should_run = false;
                let result = match state.user_db_pool.get(&user_id).await {
                    Ok(db) => sync_contact_account(&db, &state, &account_for_task).await,
                    Err(_) => Err(ProviderError {
                        category: ProviderErrorCategory::Unavailable,
                        message: "contact cache unavailable".to_owned(),
                        retry_after_seconds: None,
                    }),
                };
                let (status, retry_delay, pause) = match result {
                    Ok(()) => {
                        consecutive_failures = 0;
                        (
                            ContactSyncStatus {
                                state: "idle".to_owned(),
                                last_synced_at: Some(Utc::now().to_rfc3339()),
                                error: None,
                            },
                            None,
                            false,
                        )
                    }
                    Err(error) => {
                        consecutive_failures = consecutive_failures.saturating_add(1);
                        let pause = pauses_contact_sync(error.category);
                        let error_code = provider_error_code(&error);
                        let retry_delay = (!pause).then(|| {
                            retry_policy.delay(consecutive_failures, error.retry_after_seconds)
                        });
                        let state_name = match error.category {
                            ProviderErrorCategory::ConsentRequired => "consent_required",
                            ProviderErrorCategory::ReauthenticationRequired
                            | ProviderErrorCategory::Authentication => "reauth_required",
                            ProviderErrorCategory::Unavailable => "unavailable",
                            _ => "error",
                        };
                        tracing::info!(
                            source_id = account_for_task,
                            operation = "contact_sync_retry_policy",
                            error_category = ?error.category,
                            error_code,
                            retry_action = if pause { "paused" } else { "scheduled" },
                            retry_in_seconds = ?retry_delay.map(|delay| delay.as_secs()),
                            provider_retry_after_seconds = ?error.retry_after_seconds,
                            "contact sync retry policy applied"
                        );
                        (
                            ContactSyncStatus {
                                state: state_name.to_owned(),
                                last_synced_at: None,
                                error: Some(error_code.to_owned()),
                            },
                            retry_delay,
                            pause,
                        )
                    }
                };
                manager_for_task
                    .update_status(&account_for_task, status.clone())
                    .await;
                publish_contact_status(&state, &user_id, &account_for_task, &status);
                if pause {
                    break;
                }
                if let Some(delay) = retry_delay {
                    tokio::time::sleep(delay).await;
                    should_run = true;
                }
            }
        })
        .await;
}

fn pauses_contact_sync(category: ProviderErrorCategory) -> bool {
    matches!(
        category,
        ProviderErrorCategory::ConsentRequired
            | ProviderErrorCategory::ReauthenticationRequired
            | ProviderErrorCategory::Authentication
            | ProviderErrorCategory::Unavailable
            | ProviderErrorCategory::InvalidResponse
    )
}

fn publish_contact_status(
    state: &AppState,
    user_id: &str,
    source_id: &str,
    status: &ContactSyncStatus,
) {
    let _ = state.events.send(crate::state::UserEvent {
        user_id: user_id.to_owned(),
        event_type: "contact_sync".to_owned(),
        payload: serde_json::json!({
            "source_id": source_id,
            "state": status.state,
            "last_synced_at": status.last_synced_at,
            "error_category": status.error,
        })
        .to_string(),
    });
}

async fn sync_contact_account_inner(
    db: &sqlx::SqlitePool,
    state: &AppState,
    account_id: &str,
) -> Result<(), ProviderError> {
    let (enabled, capability_state): (bool, String) =
        sqlx::query_as("SELECT enabled, capability_state FROM contact_accounts WHERE id = ?")
            .bind(account_id)
            .fetch_one(db)
            .await
            .map_err(cache_error)?;
    if !enabled
        || matches!(
            capability_state.as_str(),
            "disabled" | "consent_required" | "reauth_required" | "unavailable"
        )
    {
        return Err(ProviderError {
            category: match capability_state.as_str() {
                "consent_required" => ProviderErrorCategory::ConsentRequired,
                "reauth_required" => ProviderErrorCategory::ReauthenticationRequired,
                _ => ProviderErrorCategory::Unavailable,
            },
            message: "contact source is not eligible for synchronization".to_owned(),
            retry_after_seconds: None,
        });
    }

    let adapter = contact_adapter(db, state, account_id).await?;
    sync_source_once(db, account_id, adapter).await?;
    Ok(())
}

async fn contact_adapter(
    db: &sqlx::SqlitePool,
    state: &AppState,
    account_id: &str,
) -> Result<std::sync::Arc<dyn ContactProviderAdapter>, ProviderError> {
    contact_adapter_with_tls_override(db, state, account_id, None).await
}

async fn contact_adapter_with_tls_override(
    db: &sqlx::SqlitePool,
    state: &AppState,
    account_id: &str,
    accept_invalid_tls_override: Option<bool>,
) -> Result<std::sync::Arc<dyn ContactProviderAdapter>, ProviderError> {
    let (provider, base_url, auth_scheme, encrypted, email_account_id): (
        String,
        Option<String>,
        String,
        Vec<u8>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT type, base_url, auth_scheme, credentials_encrypted, email_account_id
         FROM contact_accounts WHERE id = ?",
    )
    .bind(account_id)
    .fetch_one(db)
    .await
    .map_err(cache_error)?;
    let adapter: std::sync::Arc<dyn ContactProviderAdapter> =
        if let Some(mailbox_id) = email_account_id.as_deref() {
            match provider.as_str() {
                "cardav" => {
                    let context = crate::contact_reconcile::managed_carddav_context(
                        db,
                        &state.credential_key,
                        account_id,
                    )
                    .await
                    .map_err(|_| provider_setup_error("managed CardDAV setup is incomplete"))?;
                    std::sync::Arc::new(CardDavAdapter::with_tls_options(
                        context.base_url,
                        context.auth,
                        context.trusted_cert_der.as_deref(),
                        accept_invalid_tls_override.unwrap_or(context.accept_invalid_tls),
                    )?)
                }
                "google" => std::sync::Arc::new(GooglePeopleAdapter::new(
                    managed_oauth_token(state, db, mailbox_id).await?,
                )),
                "graph" => std::sync::Arc::new(MicrosoftGraphAdapter::new(
                    managed_oauth_token(state, db, mailbox_id).await?,
                )),
                _ => return Err(provider_setup_error("unsupported contact provider")),
            }
        } else {
            let creds = decrypt_credentials(state, &encrypted).map_err(|_| {
                provider_setup_error("independent contact credentials could not be decrypted")
            })?;
            match provider.as_str() {
                "cardav" => {
                    let base = base_url
                        .ok_or_else(|| provider_setup_error("CardDAV base URL is required"))?;
                    let auth = dav_auth(&auth_scheme, &creds)
                        .map_err(|_| provider_setup_error("CardDAV credentials are incomplete"))?;
                    std::sync::Arc::new(CardDavAdapter::new(reqwest::Client::new(), base, auth))
                }
                "google" => std::sync::Arc::new(GooglePeopleAdapter::new(
                    creds["access_token"]
                        .as_str()
                        .ok_or_else(|| provider_setup_error("OAuth token is missing"))?,
                )),
                "graph" => std::sync::Arc::new(MicrosoftGraphAdapter::new(
                    creds["access_token"]
                        .as_str()
                        .ok_or_else(|| provider_setup_error("OAuth token is missing"))?,
                )),
                _ => return Err(provider_setup_error("unsupported contact provider")),
            }
        };
    Ok(adapter)
}

async fn managed_oauth_token(
    state: &AppState,
    db: &sqlx::SqlitePool,
    mailbox_id: &str,
) -> Result<String, ProviderError> {
    crate::oauth_tokens::fresh_contact_access_token(&state.credential_key, db, mailbox_id)
        .await
        .map_err(|error| ProviderError {
            category: match error {
                crate::oauth_tokens::ContactTokenError::ConsentRequired => {
                    ProviderErrorCategory::ConsentRequired
                }
                crate::oauth_tokens::ContactTokenError::ReauthenticationRequired => {
                    ProviderErrorCategory::ReauthenticationRequired
                }
                crate::oauth_tokens::ContactTokenError::NotOAuth => {
                    ProviderErrorCategory::Authentication
                }
                crate::oauth_tokens::ContactTokenError::Temporary => {
                    ProviderErrorCategory::Transport
                }
            },
            message: "contact authorization is not currently available".to_owned(),
            retry_after_seconds: None,
        })
}

fn provider_setup_error(message: impl Into<String>) -> ProviderError {
    ProviderError {
        category: ProviderErrorCategory::InvalidResponse,
        message: message.into(),
        retry_after_seconds: None,
    }
}

fn provider_app_error(error: ProviderError) -> AppError {
    match error.category {
        ProviderErrorCategory::Conflict => AppError::Conflict(error.message),
        ProviderErrorCategory::ConsentRequired
        | ProviderErrorCategory::ReauthenticationRequired
        | ProviderErrorCategory::Authentication => AppError::Unprocessable(error.message),
        _ => AppError::BadGateway(error.message),
    }
}

fn provider_discovery_app_error(error: ProviderError) -> AppError {
    if error.category == ProviderErrorCategory::Unavailable
        && error.message == CARDDAV_TLS_VERIFICATION_FAILED
    {
        return AppError::BadGatewayWithCode {
            code: "carddav_tls_certificate_invalid",
            message: error.message,
        };
    }
    match error.category {
        ProviderErrorCategory::ConsentRequired
        | ProviderErrorCategory::ReauthenticationRequired
        | ProviderErrorCategory::Authentication
        | ProviderErrorCategory::Unavailable
        | ProviderErrorCategory::InvalidResponse => AppError::Unprocessable(error.message),
        _ => provider_app_error(error),
    }
}

fn cache_error(_error: sqlx::Error) -> ProviderError {
    ProviderError {
        category: ProviderErrorCategory::Unavailable,
        message: "contact cache operation failed".to_owned(),
        retry_after_seconds: None,
    }
}

async fn fetch_account(db: &sqlx::SqlitePool, id: &str) -> Result<ContactAccount, AppError> {
    sqlx::query_as(
        "SELECT id, display_name, type AS account_type, base_url, auth_scheme, sync_token, last_synced_at, sync_status, sync_error, \
                email_account_id, management_mode, capability_state, capability_reason, enabled, cache_retained \
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
    let row: ContactRow = sqlx::query_as(&format!(
        "SELECT {SELECT_CONTACT} FROM contacts WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound)?;
    enrich_contact(db, row_to_contact(row)?).await
}

async fn fetch_contact_by_remote_id(
    db: &sqlx::SqlitePool,
    account_id: &str,
    remote_id: &str,
) -> Result<Contact, AppError> {
    let row: ContactRow = sqlx::query_as(&format!(
        "SELECT {SELECT_CONTACT} FROM contacts WHERE account_id = ? AND uid = ?"
    ))
    .bind(account_id)
    .bind(remote_id)
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound)?;
    enrich_contact(db, row_to_contact(row)?).await
}

async fn contact_page(
    db: &sqlx::SqlitePool,
    query: &ContactQuery,
    autocomplete: bool,
) -> Result<ContactPage, AppError> {
    let term = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|term| !term.is_empty());
    let like = term.map(|term| format!("%{term}%"));
    let prefix = term.map(|term| format!("{term}%"));
    let limit = query
        .limit
        .unwrap_or(if autocomplete { 10 } else { 50 })
        .clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    let where_clause = "(? IS NULL OR c.account_id = ?)
        AND (? IS NULL OR ca.email_account_id = ?)
        AND (? IS NULL OR c.book_id = ?)
        AND (? IS NULL OR EXISTS (
            SELECT 1 FROM contact_group_members AS cgm
            WHERE cgm.contact_id = c.id AND cgm.group_id = ?
        ))
        AND (? IS NULL OR c.display_name LIKE ? OR c.given_name LIKE ?
             OR c.family_name LIKE ? OR c.emails LIKE ? OR c.org LIKE ?)";
    let sql = format!(
        "SELECT {SELECT_CONTACT_QUALIFIED}
         FROM contacts AS c
         JOIN contact_accounts AS ca ON ca.id = c.account_id
         WHERE {where_clause}
         ORDER BY
           CASE WHEN ? IS NOT NULL AND ca.email_account_id = ? THEN 0 ELSE 1 END,
           CASE WHEN ? IS NOT NULL AND c.display_name LIKE ? THEN 0
                WHEN ? IS NOT NULL AND c.given_name LIKE ? THEN 1
                WHEN ? IS NOT NULL AND c.family_name LIKE ? THEN 2
                WHEN ? IS NOT NULL AND c.emails LIKE ? THEN 3 ELSE 4 END,
           c.display_name COLLATE NOCASE ASC, c.id ASC
         LIMIT ? OFFSET ?"
    );
    let rows: Vec<ContactRow> = sqlx::query_as(&sql)
        .bind(query.account_id.as_deref())
        .bind(query.account_id.as_deref())
        .bind(query.mailbox_id.as_deref())
        .bind(query.mailbox_id.as_deref())
        .bind(query.book_id.as_deref())
        .bind(query.book_id.as_deref())
        .bind(query.group_id.as_deref())
        .bind(query.group_id.as_deref())
        .bind(like.as_deref())
        .bind(like.as_deref())
        .bind(like.as_deref())
        .bind(like.as_deref())
        .bind(like.as_deref())
        .bind(like.as_deref())
        .bind(query.mailbox_id.as_deref())
        .bind(query.mailbox_id.as_deref())
        .bind(prefix.as_deref())
        .bind(prefix.as_deref())
        .bind(prefix.as_deref())
        .bind(prefix.as_deref())
        .bind(prefix.as_deref())
        .bind(prefix.as_deref())
        .bind(prefix.as_deref())
        .bind(prefix.as_deref())
        .bind(limit)
        .bind(offset)
        .fetch_all(db)
        .await?;
    let count_sql = format!(
        "SELECT count(*) FROM contacts AS c
         JOIN contact_accounts AS ca ON ca.id = c.account_id
         WHERE {where_clause}"
    );
    let total: i64 = sqlx::query_scalar(&count_sql)
        .bind(query.account_id.as_deref())
        .bind(query.account_id.as_deref())
        .bind(query.mailbox_id.as_deref())
        .bind(query.mailbox_id.as_deref())
        .bind(query.book_id.as_deref())
        .bind(query.book_id.as_deref())
        .bind(query.group_id.as_deref())
        .bind(query.group_id.as_deref())
        .bind(like.as_deref())
        .bind(like.as_deref())
        .bind(like.as_deref())
        .bind(like.as_deref())
        .bind(like.as_deref())
        .bind(like.as_deref())
        .fetch_one(db)
        .await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(enrich_contact(db, row_to_contact(row)?).await?);
    }
    Ok(ContactPage {
        items,
        total,
        limit,
        offset,
    })
}

async fn enrich_contact(db: &sqlx::SqlitePool, mut contact: Contact) -> Result<Contact, AppError> {
    let source: (Option<String>, String, String, bool, bool) = sqlx::query_as(
        "SELECT email_account_id, type, capability_state, enabled,
                CASE WHEN enabled = 1 AND capability_state IN ('idle', 'pending') THEN 1 ELSE 0 END
         FROM contact_accounts WHERE id = ?",
    )
    .bind(&contact.account_id)
    .fetch_one(db)
    .await?;
    let book_writable: bool = if let Some(book_id) = contact.book_id.as_deref() {
        sqlx::query_scalar("SELECT is_writable FROM contact_books WHERE id = ?")
            .bind(book_id)
            .fetch_optional(db)
            .await?
            .unwrap_or(false)
    } else {
        false
    };
    let groups: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT g.id, g.name, g.remote_id
         FROM contact_groups AS g
         JOIN contact_group_members AS membership ON membership.group_id = g.id
         WHERE membership.contact_id = ? ORDER BY g.name COLLATE NOCASE",
    )
    .bind(&contact.id)
    .fetch_all(db)
    .await?;
    contact.source_email_account_id = source.0;
    contact.source_provider = source.1;
    contact.source_state = source.2;
    contact.source_enabled = source.3;
    contact.source_writable = source.4 && book_writable;
    contact.groups = groups
        .into_iter()
        .map(|(id, name, remote_id)| ContactGroup {
            id,
            name,
            remote_id,
        })
        .collect();
    Ok(contact)
}

struct WritableBook {
    identity: contact_sync::ContactBookIdentity,
}

async fn writable_book(
    db: &sqlx::SqlitePool,
    account_id: &str,
    requested_book_id: Option<&str>,
) -> Result<WritableBook, AppError> {
    let row: Option<(String, String, String, Option<String>, bool, bool, String)> =
        if let Some(id) = requested_book_id {
            sqlx::query_as(
                "SELECT id, remote_id, display_name, parent_remote_id, is_default, is_writable,
                    provider_metadata
             FROM contact_books WHERE id = ? AND account_id = ?",
            )
            .bind(id)
            .bind(account_id)
            .fetch_optional(db)
            .await?
        } else {
            sqlx::query_as(
                "SELECT id, remote_id, display_name, parent_remote_id, is_default, is_writable,
                    provider_metadata
             FROM contact_books
             WHERE account_id = ? AND is_writable = 1
             ORDER BY is_default DESC, display_name COLLATE NOCASE ASC LIMIT 1",
            )
            .bind(account_id)
            .fetch_optional(db)
            .await?
        };
    let (_id, remote_id, display_name, parent_remote_id, is_default, is_writable, metadata) =
        row.ok_or_else(|| AppError::Unprocessable("no writable contact book is available".into()))?;
    if !is_writable {
        return Err(AppError::Conflict(
            "the selected contact book is read-only".into(),
        ));
    }
    Ok(WritableBook {
        identity: contact_sync::ContactBookIdentity {
            remote_id,
            display_name,
            parent_remote_id,
            is_default,
            is_writable,
            provider_metadata: serde_json::from_str(&metadata).unwrap_or(Value::Null),
        },
    })
}

async fn contact_book(
    db: &sqlx::SqlitePool,
    account_id: &str,
    book_id: &str,
) -> Result<WritableBook, AppError> {
    let row: Option<(String, String, String, Option<String>, bool, bool, String)> = sqlx::query_as(
        "SELECT id, remote_id, display_name, parent_remote_id, is_default, is_writable,
                provider_metadata
         FROM contact_books WHERE id = ? AND account_id = ?",
    )
    .bind(book_id)
    .bind(account_id)
    .fetch_optional(db)
    .await?;
    let (_id, remote_id, display_name, parent_remote_id, is_default, is_writable, metadata) =
        row.ok_or(AppError::NotFound)?;
    Ok(WritableBook {
        identity: contact_sync::ContactBookIdentity {
            remote_id,
            display_name,
            parent_remote_id,
            is_default,
            is_writable,
            provider_metadata: serde_json::from_str(&metadata).unwrap_or(Value::Null),
        },
    })
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
        book_id: row.book_id,
        remote_version: row.remote_version,
        photo_reference: row.photo_reference,
        photo_version: row.photo_version,
        photo_content_type: row.photo_content_type,
        source_email_account_id: None,
        source_provider: String::new(),
        source_state: "idle".to_owned(),
        source_enabled: true,
        source_writable: false,
        groups: Vec::new(),
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
            creds["access_token"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        ));
    }
    Ok(contact_sync::DavAuth::Basic {
        username: creds["username"].as_str().unwrap_or_default().to_owned(),
        password: creds["password"].as_str().unwrap_or_default().to_owned(),
    })
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
            .find_map(|part| {
                part.strip_prefix("MEDIATYPE=")
                    .or_else(|| part.strip_prefix("TYPE="))
            })
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn database() -> sqlx::SqlitePool {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db::migrations::run_mail_migrations(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO email_accounts
             (id, display_name, primary_email, imap_host, imap_port, imap_auth_scheme,
              smtp_host, smtp_port, smtp_auth_scheme, credentials_encrypted)
             VALUES ('mail-a', 'A', 'a@example.com', 'imap', 993, 'plain', 'smtp', 465, 'plain', X'00'),
                    ('mail-b', 'B', 'b@example.com', 'imap', 993, 'plain', 'smtp', 465, 'plain', X'00');
             INSERT INTO contact_accounts
             (id, display_name, type, credentials_encrypted, email_account_id, management_mode,
              capability_state, enabled)
             VALUES ('source-a', 'A contacts', 'cardav', X'', 'mail-a', 'mailbox', 'idle', 1),
                    ('source-b', 'B contacts', 'cardav', X'', 'mail-b', 'mailbox', 'disabled', 0);
             INSERT INTO contact_books
             (id, account_id, remote_id, display_name, is_default, is_writable)
             VALUES ('book-a', 'source-a', 'remote-a', 'Personal', 1, 1),
                    ('book-b', 'source-b', 'remote-b', 'Work', 1, 1);
             INSERT INTO contacts
             (id, account_id, book_id, uid, display_name, emails)
             VALUES ('contact-a', 'source-a', 'book-a', 'a', 'Alice', '[{\"value\":\"alice@example.com\"}]'),
                    ('contact-b', 'source-b', 'book-b', 'b', 'Bob', '[{\"value\":\"bob@example.com\"}]');
             INSERT INTO contact_groups (id, account_id, book_id, name, remote_id)
             VALUES ('friends', 'source-a', 'book-a', 'Friends', 'group/friends');
             INSERT INTO contact_group_members (contact_id, group_id)
             VALUES ('contact-a', 'friends');
             INSERT INTO folders (id, account_id, name, full_path, folder_type)
             VALUES ('folder-a', 'mail-a', 'Inbox', 'INBOX', 'INBOX'),
                    ('folder-b', 'mail-b', 'Inbox', 'INBOX', 'INBOX');
             INSERT INTO messages (id, account_id, folder_id, uid, from_addr, internal_date)
             VALUES ('message-a1', 'mail-a', 'folder-a', 1, 'Alice Recent <alice@example.com>', '2026-01-01T00:00:00Z'),
                    ('message-a2', 'mail-a', 'folder-a', 2, 'Alicia Sender <alicia@example.net>', '2026-01-02T00:00:00Z'),
                    ('message-b1', 'mail-b', 'folder-b', 1, 'Alistair Remote <alistair@example.net>', '2026-01-03T00:00:00Z')",
        )
        .execute(&db)
        .await
        .unwrap();
        db
    }

    #[test]
    fn deterministic_contact_sync_failures_pause_automatic_retries() {
        for category in [
            ProviderErrorCategory::ConsentRequired,
            ProviderErrorCategory::ReauthenticationRequired,
            ProviderErrorCategory::Authentication,
            ProviderErrorCategory::Unavailable,
            ProviderErrorCategory::InvalidResponse,
        ] {
            assert!(pauses_contact_sync(category));
        }

        for category in [
            ProviderErrorCategory::Transport,
            ProviderErrorCategory::RateLimited,
            ProviderErrorCategory::Conflict,
            ProviderErrorCategory::CursorExpired,
        ] {
            assert!(!pauses_contact_sync(category));
        }
    }

    #[test]
    fn discovery_reports_structured_tls_and_transient_provider_failures() {
        let correctable = provider_discovery_app_error(ProviderError {
            category: ProviderErrorCategory::Unavailable,
            message: CARDDAV_TLS_VERIFICATION_FAILED.to_owned(),
            retry_after_seconds: None,
        });
        assert!(matches!(
            correctable,
            AppError::BadGatewayWithCode {
                code: "carddav_tls_certificate_invalid",
                ..
            }
        ));

        let transient = provider_discovery_app_error(ProviderError {
            category: ProviderErrorCategory::Transport,
            message: "CardDAV server could not be reached".to_owned(),
            retry_after_seconds: None,
        })
        .into_response();
        assert_eq!(transient.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn contact_page_filters_by_mailbox_and_returns_stable_counts_and_groups() {
        let db = database().await;
        let page = contact_page(
            &db,
            &ContactQuery {
                q: Some("ali".into()),
                account_id: None,
                mailbox_id: Some("mail-a".into()),
                book_id: Some("book-a".into()),
                group_id: None,
                limit: Some(20),
                offset: Some(0),
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].display_name.as_deref(), Some("Alice"));
        assert_eq!(page.items[0].groups[0].name, "Friends");
        assert_eq!(page.items[0].source_provider, "cardav");
        assert!(page.items[0].source_writable);
    }

    #[tokio::test]
    async fn explicit_carddav_tls_decision_updates_only_the_owning_mailbox() {
        let db = database().await;

        enable_mailbox_dav_tls_exception(&db, "mail-a", "source-a")
            .await
            .unwrap();

        let accepted: bool = sqlx::query_scalar(
            "SELECT caldav_accept_invalid_tls FROM email_accounts WHERE id = 'mail-a'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert!(accepted);
        assert!(enable_mailbox_dav_tls_exception(&db, "mail-a", "source-b")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn contact_groups_return_resolved_names_counts_and_filter_contacts() {
        let db = database().await;
        let groups = contact_groups(
            &db,
            &ContactGroupQuery {
                account_id: Some("source-a".into()),
                mailbox_id: None,
                book_id: Some("book-a".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "Friends");
        assert_eq!(groups[0].member_count, 1);

        let page = contact_page(
            &db,
            &ContactQuery {
                q: None,
                account_id: None,
                mailbox_id: None,
                book_id: None,
                group_id: Some("friends".into()),
                limit: Some(20),
                offset: Some(0),
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].display_name.as_deref(), Some("Alice"));
    }

    #[tokio::test]
    async fn recipient_suggestions_merge_contacts_and_known_senders() {
        let db = database().await;
        let suggestions = recipient_suggestion_rows(
            &db,
            &ContactQuery {
                q: Some("ali".into()),
                account_id: None,
                mailbox_id: Some("mail-b".into()),
                book_id: None,
                group_id: None,
                limit: None,
                offset: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(suggestions[0].email, "alice@example.com");
        assert_eq!(suggestions[0].source, "contact");
        assert_eq!(suggestions[1].email, "alistair@example.net");
        assert_eq!(suggestions[1].source, "sender");
        assert_eq!(suggestions[2].email, "alicia@example.net");
        assert_eq!(suggestions[2].source, "sender");
        assert_eq!(
            suggestions
                .iter()
                .filter(|suggestion| suggestion.email == "alice@example.com")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn retained_disabled_cache_is_visible_but_read_only() {
        let db = database().await;
        let page = contact_page(
            &db,
            &ContactQuery {
                q: None,
                account_id: Some("source-b".into()),
                mailbox_id: None,
                book_id: None,
                group_id: None,
                limit: Some(20),
                offset: Some(0),
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].source_state, "disabled");
        assert!(!page.items[0].source_writable);
    }

    #[tokio::test]
    async fn autocomplete_prefers_mailbox_context_and_user_databases_are_isolated() {
        let first_user = database().await;
        let second_user = database().await;
        sqlx::query(
            "UPDATE contacts SET display_name = 'Alex B' WHERE id = 'contact-b';
             UPDATE contacts SET display_name = 'Alex A' WHERE id = 'contact-a'",
        )
        .execute(&first_user)
        .await
        .unwrap();
        sqlx::query("DELETE FROM contacts WHERE id = 'contact-a'")
            .execute(&second_user)
            .await
            .unwrap();
        let page = contact_page(
            &first_user,
            &ContactQuery {
                q: Some("Alex".into()),
                account_id: None,
                mailbox_id: Some("mail-b".into()),
                book_id: None,
                group_id: None,
                limit: Some(10),
                offset: Some(0),
            },
            true,
        )
        .await
        .unwrap();
        assert_eq!(
            page.items[0].source_email_account_id.as_deref(),
            Some("mail-b")
        );

        let isolated_count: i64 = sqlx::query_scalar("SELECT count(*) FROM contacts")
            .fetch_one(&second_user)
            .await
            .unwrap();
        assert_eq!(isolated_count, 1);
    }
}
