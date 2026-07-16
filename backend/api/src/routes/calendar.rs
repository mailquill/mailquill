use axum::{
    extract::{Extension, Path, Query, State},
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{error::AppError, middleware::UserId, state::AppState};

#[derive(Serialize, sqlx::FromRow)]
pub struct CalendarAccount {
    id: String,
    display_name: String,
    #[serde(rename = "type")]
    account_type: String,
    base_url: Option<String>,
    auth_scheme: String,
    sync_interval_secs: i64,
    last_synced_at: Option<String>,
    sync_status: String,
    sync_error: Option<String>,
}

#[derive(Deserialize)]
pub struct NewCalendarAccount {
    display_name: String,
    #[serde(rename = "type")]
    account_type: String,
    base_url: Option<String>,
    auth_scheme: Option<String>,
    username: Option<String>,
    password: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
    accept_invalid_tls: Option<bool>,
    sync_interval_secs: Option<i64>,
}

#[derive(Serialize)]
pub struct SyncStatus {
    status: String,
    last_synced_at: Option<String>,
    error: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Calendar {
    id: String,
    account_id: Option<String>,
    name: String,
    color: String,
    is_default: bool,
    dav_url: Option<String>,
    created_at: String,
    provider_type: Option<String>,
}

#[derive(Deserialize)]
pub struct NewCalendar {
    account_id: Option<String>,
    name: String,
    color: Option<String>,
    dav_url: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct CalendarEvent {
    id: String,
    calendar_id: String,
    title: String,
    description: Option<String>,
    location: Option<String>,
    starts_at: String,
    ends_at: String,
    all_day: bool,
    color: String,
    rrule: Option<String>,
    rrule_uid: Option<String>,
    recurrence_id: Option<String>,
    status: String,
    organizer_email: Option<String>,
    organizer_name: Option<String>,
    attendees: String,
    ms_busystatus: Option<String>,
    ms_teams_url: Option<String>,
    raw_ical: Option<String>,
}

#[derive(Deserialize)]
pub struct NewEvent {
    calendar_id: String,
    title: String,
    description: Option<String>,
    location: Option<String>,
    starts_at: String,
    ends_at: String,
    all_day: Option<bool>,
    rrule: Option<String>,
    attendees: Option<Value>,
    organizer_email: Option<String>,
    organizer_name: Option<String>,
    recurring_edit_scope: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateEvent {
    title: String,
    description: Option<String>,
    location: Option<String>,
    starts_at: String,
    ends_at: String,
    all_day: Option<bool>,
    rrule: Option<String>,
    attendees: Option<Value>,
    organizer_email: Option<String>,
    organizer_name: Option<String>,
    recurring_edit_scope: Option<String>,
}

#[derive(Deserialize)]
pub struct EventRange {
    from: Option<String>,
    to: Option<String>,
    start: Option<String>,
    end: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct MeetingInvitation {
    id: String,
    message_id: String,
    method: String,
    uid: String,
    summary: Option<String>,
    start_dt: Option<String>,
    end_dt: Option<String>,
    organizer_email: Option<String>,
    attendees: String,
    user_rsvp_status: String,
    raw_ical: String,
    ms_teams_url: Option<String>,
}

#[derive(Deserialize)]
pub struct InvitationQuery {
    message_id: Option<String>,
    status: Option<String>,
}

#[derive(Deserialize)]
pub struct RsvpRequest {
    response: String,
}

pub async fn create_account(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<NewCalendarAccount>,
) -> Result<impl IntoResponse, AppError> {
    validate_provider(&req)?;
    let user_db = state.user_db_pool.get(&user.0).await?;
    let auth_scheme = req.auth_scheme.clone().unwrap_or_else(|| {
        if req.access_token.is_some() {
            "oauth2".into()
        } else {
            "basic".into()
        }
    });
    let credentials = serde_json::json!({
        "username": req.username,
        "password": req.password,
        "access_token": req.access_token,
        "refresh_token": req.refresh_token,
        "accept_invalid_tls": req.accept_invalid_tls.unwrap_or(false),
    });
    let encrypted = state
        .credential_key
        .encrypt(
            serde_json::to_vec(&credentials)
                .map_err(|e| AppError::Internal(e.to_string()))?
                .as_slice(),
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let id: String = sqlx::query_scalar(
        "INSERT INTO calendar_accounts \
         (display_name, type, base_url, auth_scheme, credentials_encrypted, sync_interval_secs, sync_status) \
         VALUES (?, ?, ?, ?, ?, ?, 'syncing') RETURNING id",
    )
    .bind(&req.display_name)
    .bind(&req.account_type)
    .bind(&req.base_url)
    .bind(&auth_scheme)
    .bind(&encrypted)
    .bind(req.sync_interval_secs.unwrap_or(300))
    .fetch_one(&user_db)
    .await?;

    if let Err(err) = sync_calendar_account(&user_db, &state, &id).await {
        let _ = sqlx::query("DELETE FROM calendar_accounts WHERE id = ?")
            .bind(&id)
            .execute(&user_db)
            .await;
        return Err(if req.account_type == "caldav" {
            super::caldav_error::curated_caldav_error(err)
        } else {
            AppError::BadGateway(err)
        });
    }

    let account = fetch_account(&user_db, &id).await?;
    Ok(Json(account))
}

/// Link a Google Calendar account to its Gmail OAuth account and run the first sync.
pub(crate) async fn connect_google_account(
    db: &sqlx::SqlitePool,
    state: &AppState,
    account_id: &str,
    display_name: &str,
    encrypted_credentials: &[u8],
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO calendar_accounts \
         (id, display_name, type, base_url, auth_scheme, credentials_encrypted, sync_interval_secs, sync_status) \
         VALUES (?, ?, 'google', NULL, 'oauth2', ?, 300, 'syncing') \
         ON CONFLICT(id) DO UPDATE SET display_name=excluded.display_name, type='google', \
         auth_scheme='oauth2', credentials_encrypted=excluded.credentials_encrypted, sync_status='syncing', sync_error=NULL",
    )
    .bind(account_id)
    .bind(display_name)
    .bind(encrypted_credentials)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;

    sync_calendar_account(db, state, account_id).await
}

pub async fn list_accounts(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let accounts: Vec<CalendarAccount> = sqlx::query_as(
        "SELECT id, display_name, type AS account_type, base_url, auth_scheme, sync_interval_secs, last_synced_at, sync_status, sync_error \
         FROM calendar_accounts ORDER BY display_name COLLATE NOCASE ASC",
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
    sqlx::query("DELETE FROM calendar_events WHERE calendar_id IN (SELECT id FROM calendars WHERE account_id = ?)")
        .bind(&id)
        .execute(&user_db)
        .await?;
    sqlx::query("DELETE FROM calendars WHERE account_id = ?")
        .bind(&id)
        .execute(&user_db)
        .await?;
    let rows = sqlx::query("DELETE FROM calendar_accounts WHERE id = ?")
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
        "SELECT sync_status, last_synced_at, sync_error FROM calendar_accounts WHERE id = ?",
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

pub async fn sync_account(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    sync_calendar_account(&user_db, &state, &id)
        .await
        .map_err(AppError::BadGateway)?;
    account_sync_status(State(state), Extension(user), Path(id)).await
}

pub async fn list_calendars(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let calendars: Vec<Calendar> = sqlx::query_as(
        "SELECT c.id, c.account_id, c.name, c.color, c.is_default, c.dav_url, c.created_at, a.type AS provider_type \
         FROM calendars c LEFT JOIN calendar_accounts a ON a.id = c.account_id ORDER BY c.name COLLATE NOCASE ASC",
    )
    .fetch_all(&user_db)
    .await?;
    Ok(Json(calendars))
}

pub async fn create_calendar(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<NewCalendar>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let id: String = sqlx::query_scalar(
        "INSERT INTO calendars (account_id, name, color, dav_url) VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(&req.account_id)
    .bind(&req.name)
    .bind(req.color.as_deref().unwrap_or("#2563EB"))
    .bind(&req.dav_url)
    .fetch_one(&user_db)
    .await?;
    let calendar: Calendar = sqlx::query_as(
        "SELECT c.id, c.account_id, c.name, c.color, c.is_default, c.dav_url, c.created_at, a.type AS provider_type \
         FROM calendars c LEFT JOIN calendar_accounts a ON a.id = c.account_id WHERE c.id = ?",
    )
    .bind(&id)
    .fetch_one(&user_db)
    .await?;
    Ok(Json(calendar))
}

pub async fn update_calendar(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
    Json(req): Json<UpdateCalendar>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    sqlx::query(
        "UPDATE calendars SET name = COALESCE(?, name), color = COALESCE(?, color) WHERE id = ?",
    )
    .bind(&req.name)
    .bind(&req.color)
    .bind(&id)
    .execute(&user_db)
    .await?;
    let calendar: Calendar = sqlx::query_as(
        "SELECT c.id, c.account_id, c.name, c.color, c.is_default, c.dav_url, c.created_at, a.type AS provider_type \
         FROM calendars c LEFT JOIN calendar_accounts a ON a.id = c.account_id WHERE c.id = ?",
    )
    .bind(&id)
    .fetch_one(&user_db)
    .await?;
    Ok(Json(calendar))
}

#[derive(Deserialize)]
pub struct UpdateCalendar {
    name: Option<String>,
    color: Option<String>,
}

pub async fn delete_calendar(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    sqlx::query("DELETE FROM calendars WHERE id = ?")
        .bind(&id)
        .execute(&user_db)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

pub async fn list_events(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(range): Query<EventRange>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let from = range.start.or(range.from).unwrap_or_else(|| "0000".into());
    let to = range.end.or(range.to).unwrap_or_else(|| "9999".into());
    let events: Vec<CalendarEvent> = sqlx::query_as(
        "SELECT e.id, e.calendar_id, e.summary AS title, e.description, e.location, e.start_dt AS starts_at, e.end_dt AS ends_at, e.all_day, \
         COALESCE(c.color, '#2563EB') AS color, e.rrule, e.rrule_uid, e.recurrence_id, e.status, e.organizer_email, e.organizer_name, \
         e.attendees, e.ms_busystatus, e.ms_teams_url, e.raw_ical \
         FROM calendar_events e LEFT JOIN calendars c ON c.id = e.calendar_id \
         WHERE e.deleted = 0 AND e.start_dt <= ? AND e.end_dt >= ? ORDER BY e.start_dt ASC",
    )
    .bind(&to)
    .bind(&from)
    .fetch_all(&user_db)
    .await?;
    Ok(Json(events))
}

pub async fn create_event(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<NewEvent>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let attendees = req
        .attendees
        .as_ref()
        .map(Value::to_string)
        .unwrap_or_else(|| "[]".to_owned());
    let uid = format!("{}@mailquill", uuid::Uuid::new_v4());
    let raw_ical = calendar_sync::build_ics(&event_input(
        &uid,
        &req.title,
        &req.starts_at,
        &req.ends_at,
        req.all_day.unwrap_or(false),
        req.location.as_deref(),
        req.description.as_deref(),
        req.rrule.as_deref(),
        Some(&attendees),
        req.organizer_email.as_deref(),
        req.organizer_name.as_deref(),
    ));

    let (remote_id, etag) = write_remote_event(
        &user_db,
        &state,
        &req.calendar_id,
        None,
        None,
        &raw_ical,
        &req,
        false,
    )
    .await?;

    let id: String = sqlx::query_scalar(
        "INSERT INTO calendar_events \
         (calendar_id, uid, summary, description, location, start_dt, end_dt, all_day, rrule, organizer_email, organizer_name, attendees, raw_ical, href, etag, dirty, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, datetime('now')) RETURNING id",
    )
    .bind(&req.calendar_id)
    .bind(&uid)
    .bind(&req.title)
    .bind(&req.description)
    .bind(&req.location)
    .bind(&req.starts_at)
    .bind(&req.ends_at)
    .bind(req.all_day.unwrap_or(false))
    .bind(&req.rrule)
    .bind(&req.organizer_email)
    .bind(&req.organizer_name)
    .bind(&attendees)
    .bind(&raw_ical)
    .bind(&remote_id)
    .bind(&etag)
    .fetch_one(&user_db)
    .await?;
    expand_and_store_rrule(&user_db, &id).await?;
    Ok(Json(
        serde_json::json!({ "id": id, "recurring_edit_scope": req.recurring_edit_scope }),
    ))
}

pub async fn update_event(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
    Json(req): Json<UpdateEvent>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let row: Option<(String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT calendar_id, uid, href, etag FROM calendar_events WHERE id = ?")
            .bind(&id)
            .fetch_optional(&user_db)
            .await?;
    let Some((calendar_id, uid, href, etag)) = row else {
        return Err(AppError::NotFound);
    };
    let uid = uid.unwrap_or_else(|| format!("{}@mailquill", uuid::Uuid::new_v4()));
    let attendees = req
        .attendees
        .as_ref()
        .map(Value::to_string)
        .unwrap_or_else(|| "[]".to_owned());
    let raw_ical = calendar_sync::build_ics(&event_input(
        &uid,
        &req.title,
        &req.starts_at,
        &req.ends_at,
        req.all_day.unwrap_or(false),
        req.location.as_deref(),
        req.description.as_deref(),
        req.rrule.as_deref(),
        Some(&attendees),
        req.organizer_email.as_deref(),
        req.organizer_name.as_deref(),
    ));
    let shim = NewEvent {
        calendar_id: calendar_id.clone(),
        title: req.title.clone(),
        description: req.description.clone(),
        location: req.location.clone(),
        starts_at: req.starts_at.clone(),
        ends_at: req.ends_at.clone(),
        all_day: req.all_day,
        rrule: req.rrule.clone(),
        attendees: req.attendees.clone(),
        organizer_email: req.organizer_email.clone(),
        organizer_name: req.organizer_name.clone(),
        recurring_edit_scope: req.recurring_edit_scope.clone(),
    };
    let (remote_id, new_etag) = write_remote_event(
        &user_db,
        &state,
        &calendar_id,
        href.as_deref(),
        etag.as_deref(),
        &raw_ical,
        &shim,
        false,
    )
    .await?;

    sqlx::query(
        "UPDATE calendar_events SET uid=?, summary=?, description=?, location=?, start_dt=?, end_dt=?, all_day=?, rrule=?, organizer_email=?, organizer_name=?, attendees=?, raw_ical=?, href=COALESCE(?, href), etag=COALESCE(?, etag), updated_at=datetime('now') WHERE id=?",
    )
    .bind(&uid)
    .bind(&req.title)
    .bind(&req.description)
    .bind(&req.location)
    .bind(&req.starts_at)
    .bind(&req.ends_at)
    .bind(req.all_day.unwrap_or(false))
    .bind(&req.rrule)
    .bind(&req.organizer_email)
    .bind(&req.organizer_name)
    .bind(&attendees)
    .bind(&raw_ical)
    .bind(&remote_id)
    .bind(&new_etag)
    .bind(&id)
    .execute(&user_db)
    .await?;
    expand_and_store_rrule(&user_db, &id).await?;
    Ok(Json(
        serde_json::json!({ "id": id, "recurring_edit_scope": req.recurring_edit_scope }),
    ))
}

pub async fn delete_event(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let row: Option<(String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT calendar_id, href, etag FROM calendar_events WHERE id = ?")
            .bind(&id)
            .fetch_optional(&user_db)
            .await?;
    let Some((calendar_id, href, etag)) = row else {
        return Err(AppError::NotFound);
    };
    let shim = NewEvent {
        calendar_id: calendar_id.clone(),
        title: String::new(),
        description: None,
        location: None,
        starts_at: Utc::now().to_rfc3339(),
        ends_at: Utc::now().to_rfc3339(),
        all_day: Some(false),
        rrule: None,
        attendees: None,
        organizer_email: None,
        organizer_name: None,
        recurring_edit_scope: None,
    };
    let _ = write_remote_event(
        &user_db,
        &state,
        &calendar_id,
        href.as_deref(),
        etag.as_deref(),
        "",
        &shim,
        true,
    )
    .await?;
    sqlx::query("DELETE FROM calendar_events WHERE id = ? OR rrule_uid = (SELECT uid FROM calendar_events WHERE id = ?)")
        .bind(&id)
        .bind(&id)
        .execute(&user_db)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

pub async fn list_invitations(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(query): Query<InvitationQuery>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let status = query.status.unwrap_or_else(|| "pending".to_owned());
    let rows: Vec<MeetingInvitation> = if let Some(message_id) = query.message_id {
        sqlx::query_as(
            "SELECT id, message_id, method, uid, summary, start_dt, end_dt, organizer_email, attendees, user_rsvp_status, raw_ical, ms_teams_url \
             FROM meeting_invitations WHERE message_id = ? ORDER BY start_dt ASC",
        )
        .bind(message_id)
        .fetch_all(&user_db)
        .await?
    } else {
        sqlx::query_as(
            "SELECT id, message_id, method, uid, summary, start_dt, end_dt, organizer_email, attendees, user_rsvp_status, raw_ical, ms_teams_url \
             FROM meeting_invitations WHERE user_rsvp_status = ? ORDER BY start_dt ASC",
        )
        .bind(status)
        .fetch_all(&user_db)
        .await?
    };
    Ok(Json(rows))
}

pub async fn rsvp_invitation(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
    Json(req): Json<RsvpRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let response = normalize_rsvp(&req.response)?;
    let invite: Option<(String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, String, String)> =
        sqlx::query_as(
            "SELECT uid, summary, start_dt, end_dt, organizer_email, ms_teams_url, attendees, raw_ical FROM meeting_invitations WHERE id = ?",
        )
        .bind(&id)
        .fetch_optional(&user_db)
        .await?;
    let Some((uid, summary, start_dt, end_dt, organizer_email, teams_url, attendees, raw_ical)) =
        invite
    else {
        return Err(AppError::NotFound);
    };
    let partstat = match response.as_str() {
        "accepted" => "ACCEPTED",
        "tentative" => "TENTATIVE",
        _ => "DECLINED",
    };

    if response != "declined" {
        if let Some(calendar_id) = default_calendar_id(&state, &user.0, &user_db).await? {
            sqlx::query(
                "INSERT INTO calendar_events \
                 (calendar_id, uid, summary, start_dt, end_dt, attendees, organizer_email, ms_teams_url, raw_ical, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))",
            )
            .bind(&calendar_id)
            .bind(&uid)
            .bind(summary.as_deref().unwrap_or("(No title)"))
            .bind(start_dt.as_deref().unwrap_or(""))
            .bind(end_dt.as_deref().unwrap_or_else(|| start_dt.as_deref().unwrap_or("")))
            .bind(&attendees)
            .bind(&organizer_email)
            .bind(&teams_url)
            .bind(&raw_ical)
            .execute(&user_db)
            .await?;
        }
    }

    if let Some(to) = organizer_email.as_deref() {
        let _ = send_rsvp_reply(
            &state,
            &user_db,
            to,
            &uid,
            summary.as_deref().unwrap_or("Meeting response"),
            start_dt.as_deref(),
            end_dt.as_deref(),
            partstat,
        )
        .await;
    }

    sqlx::query("UPDATE meeting_invitations SET user_rsvp_status = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(&response)
        .bind(&id)
        .execute(&user_db)
        .await?;

    Ok(Json(serde_json::json!({ "id": id, "status": response })))
}

fn validate_provider(req: &NewCalendarAccount) -> Result<(), AppError> {
    match req.account_type.as_str() {
        "openxchange" if req.base_url.is_none() => {
            Err(AppError::Unprocessable("base_url is required".into()))
        }
        "caldav"
            if req
                .base_url
                .as_deref()
                .is_none_or(|base| base.trim().is_empty())
                && req
                    .username
                    .as_deref()
                    .is_none_or(|username| !username.contains('@')) =>
        {
            Err(AppError::Unprocessable(
                "CalDAV autodiscovery requires a base_url or username email".into(),
            ))
        }
        "caldav" | "openxchange" if req.password.is_none() && req.access_token.is_none() => {
            Err(AppError::Unprocessable("credentials are required".into()))
        }
        "graph" | "google" if req.access_token.is_none() => {
            Err(AppError::Unprocessable("access_token is required".into()))
        }
        "caldav" | "graph" | "google" | "openxchange" => Ok(()),
        _ => Err(AppError::Unprocessable(
            "unsupported calendar provider".into(),
        )),
    }
}

async fn fetch_account(db: &sqlx::SqlitePool, id: &str) -> Result<CalendarAccount, AppError> {
    sqlx::query_as(
        "SELECT id, display_name, type AS account_type, base_url, auth_scheme, sync_interval_secs, last_synced_at, sync_status, sync_error \
         FROM calendar_accounts WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound)
}

async fn sync_calendar_account(
    db: &sqlx::SqlitePool,
    state: &AppState,
    account_id: &str,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE calendar_accounts SET sync_status = 'syncing', sync_error = NULL WHERE id = ?",
    )
    .bind(account_id)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;
    let result = sync_calendar_account_inner(db, state, account_id).await;
    match &result {
        Ok(()) => {
            let _ = sqlx::query("UPDATE calendar_accounts SET sync_status='idle', last_synced_at=?, sync_error=NULL WHERE id=?")
                .bind(Utc::now().to_rfc3339())
                .bind(account_id)
                .execute(db)
                .await;
        }
        Err(err) => {
            let _ = sqlx::query(
                "UPDATE calendar_accounts SET sync_status='error', sync_error=? WHERE id=?",
            )
            .bind(err)
            .bind(account_id)
            .execute(db)
            .await;
        }
    }
    result
}

async fn sync_calendar_account_inner(
    db: &sqlx::SqlitePool,
    state: &AppState,
    account_id: &str,
) -> Result<(), String> {
    let row: (String, Option<String>, String, Vec<u8>, Option<String>) = sqlx::query_as(
        "SELECT type, base_url, auth_scheme, credentials_encrypted, sync_token FROM calendar_accounts WHERE id = ?",
    )
    .bind(account_id)
    .fetch_one(db)
    .await
    .map_err(|e| e.to_string())?;
    let (provider, base_url, auth_scheme, encrypted, sync_token) = row;
    let creds = decrypt_calendar_credentials(state, &encrypted)?;
    let start = Utc::now() - Duration::days(365);
    let end = Utc::now() + Duration::days(730);

    match provider.as_str() {
        "caldav" => {
            let auth = dav_auth(&auth_scheme, &creds)?;
            let accept_invalid_tls = creds["accept_invalid_tls"].as_bool().unwrap_or(false);
            let (base, calendars) =
                discover_caldav_calendars(base_url.as_deref(), &creds, &auth, accept_invalid_tls)
                    .await?;
            sqlx::query("UPDATE calendar_accounts SET base_url = ? WHERE id = ?")
                .bind(&base)
                .bind(account_id)
                .execute(db)
                .await
                .map_err(|e| e.to_string())?;
            for calendar in calendars {
                let calendar_id = upsert_calendar(db, account_id, &calendar).await?;
                let ctag = calendar_sync::collection_ctag_with_tls_options(
                    &calendar.url,
                    &auth,
                    None,
                    accept_invalid_tls,
                )
                .await
                .ok()
                .flatten();
                let previous: Option<String> =
                    sqlx::query_scalar("SELECT ctag FROM calendars WHERE id = ?")
                        .bind(&calendar_id)
                        .fetch_optional(db)
                        .await
                        .map_err(|e| e.to_string())?
                        .flatten();
                if ctag.is_some() && ctag == previous {
                    continue;
                }
                let events = calendar_sync::pull_range_with_tls_options(
                    &calendar.url,
                    &auth,
                    Some(start),
                    Some(end),
                    None,
                    accept_invalid_tls,
                )
                .await?;
                store_events(db, &calendar_id, events).await?;
                sqlx::query("UPDATE calendars SET ctag = ? WHERE id = ?")
                    .bind(&ctag)
                    .bind(&calendar_id)
                    .execute(db)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        "graph" => {
            let access_token = creds["access_token"]
                .as_str()
                .ok_or_else(|| "access_token missing".to_owned())?;
            let result =
                calendar_sync::graph_sync(access_token, start, end, sync_token.as_deref()).await?;
            store_sync_result(db, account_id, result).await?;
        }
        "google" => {
            let access_token = google_access_token(db, state, account_id, &creds).await?;
            let results = calendar_sync::google_sync(&access_token, start, end).await?;
            for result in results {
                store_sync_result(db, account_id, result).await?;
            }
        }
        "openxchange" => {
            let base = base_url.ok_or_else(|| "base_url is required".to_owned())?;
            let auth = dav_auth(&auth_scheme, &creds)?;
            let result = calendar_sync::ox_sync(&base, &auth).await?;
            store_sync_result(db, account_id, result).await?;
        }
        _ => return Err("unsupported provider".into()),
    }
    Ok(())
}

async fn store_sync_result(
    db: &sqlx::SqlitePool,
    account_id: &str,
    result: calendar_sync::SyncResult,
) -> Result<(), String> {
    let calendar = result
        .calendars
        .first()
        .cloned()
        .unwrap_or(calendar_sync::DiscoveredCalendar {
            name: "Calendar".into(),
            url: account_id.to_owned(),
            color: None,
        });
    let calendar_id = upsert_calendar(db, account_id, &calendar).await?;
    store_events(db, &calendar_id, result.events).await?;
    sqlx::query("UPDATE calendar_accounts SET sync_token = ? WHERE id = ?")
        .bind(result.sync_token)
        .bind(account_id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Resolve Google Calendar credentials through the linked Gmail account when available.
async fn google_access_token(
    db: &sqlx::SqlitePool,
    state: &AppState,
    account_id: &str,
    calendar_credentials: &Value,
) -> Result<String, String> {
    let linked_email_account: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM email_accounts WHERE id = ? AND provider_kind = 'gmail_api')",
    )
    .bind(account_id)
    .fetch_one(db)
    .await
    .map_err(|error| error.to_string())?;

    if linked_email_account {
        return crate::oauth_tokens::fresh_access_token(&state.credential_key, db, account_id)
            .await?
            .ok_or_else(|| "linked Gmail account has no OAuth access token".to_owned());
    }

    calendar_credentials["access_token"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "access_token missing".to_owned())
}

async fn upsert_calendar(
    db: &sqlx::SqlitePool,
    account_id: &str,
    calendar: &calendar_sync::DiscoveredCalendar,
) -> Result<String, String> {
    if let Some(id) = sqlx::query_scalar::<_, String>(
        "SELECT id FROM calendars WHERE account_id = ? AND dav_url = ?",
    )
    .bind(account_id)
    .bind(&calendar.url)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?
    {
        sqlx::query("UPDATE calendars SET name=?, color=COALESCE(?, color) WHERE id=?")
            .bind(&calendar.name)
            .bind(&calendar.color)
            .bind(&id)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(id);
    }
    sqlx::query_scalar(
        "INSERT INTO calendars (account_id, name, color, dav_url) VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(account_id)
    .bind(&calendar.name)
    .bind(calendar.color.as_deref().unwrap_or("#2563EB"))
    .bind(&calendar.url)
    .fetch_one(db)
    .await
    .map_err(|e| e.to_string())
}

async fn store_events(
    db: &sqlx::SqlitePool,
    calendar_id: &str,
    events: Vec<calendar_sync::ParsedEvent>,
) -> Result<(), String> {
    for event in events
        .into_iter()
        .flat_map(|event| calendar_sync::expand_rrule(&event, 730))
    {
        let attendees = event.attendees_json.unwrap_or_else(|| "[]".to_owned());
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT id FROM calendar_events WHERE calendar_id = ? AND uid = ? AND COALESCE(recurrence_id, '') = COALESCE(?, '')",
        )
        .bind(calendar_id)
        .bind(&event.uid)
        .bind(&event.recurrence_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
        if let Some(id) = existing {
            sqlx::query(
                "UPDATE calendar_events SET summary=?, description=?, location=?, start_dt=?, end_dt=?, all_day=?, rrule=?, rrule_uid=?, recurrence_id=?, status=COALESCE(?, status), organizer_email=?, organizer_name=?, attendees=?, ms_busystatus=?, ms_teams_url=?, raw_ical=?, href=?, etag=?, synced_at=?, dirty=0, deleted=0, updated_at=datetime('now') WHERE id=?",
            )
            .bind(&event.title)
            .bind(&event.description)
            .bind(&event.location)
            .bind(&event.starts_at)
            .bind(&event.ends_at)
            .bind(event.all_day)
            .bind(&event.rrule)
            .bind(&event.rrule_uid)
            .bind(&event.recurrence_id)
            .bind(&event.status)
            .bind(&event.organizer_email)
            .bind(&event.organizer_name)
            .bind(&attendees)
            .bind(&event.ms_busystatus)
            .bind(&event.ms_teams_url)
            .bind(&event.raw_ical)
            .bind(&event.href)
            .bind(&event.etag)
            .bind(Utc::now().to_rfc3339())
            .bind(&id)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        } else {
            sqlx::query(
                "INSERT INTO calendar_events (calendar_id, uid, summary, description, location, start_dt, end_dt, all_day, rrule, rrule_uid, recurrence_id, status, organizer_email, organizer_name, attendees, ms_busystatus, ms_teams_url, raw_ical, href, etag, synced_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, COALESCE(?, 'confirmed'), ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(calendar_id)
            .bind(&event.uid)
            .bind(&event.title)
            .bind(&event.description)
            .bind(&event.location)
            .bind(&event.starts_at)
            .bind(&event.ends_at)
            .bind(event.all_day)
            .bind(&event.rrule)
            .bind(&event.rrule_uid)
            .bind(&event.recurrence_id)
            .bind(&event.status)
            .bind(&event.organizer_email)
            .bind(&event.organizer_name)
            .bind(&attendees)
            .bind(&event.ms_busystatus)
            .bind(&event.ms_teams_url)
            .bind(&event.raw_ical)
            .bind(&event.href)
            .bind(&event.etag)
            .bind(Utc::now().to_rfc3339())
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

async fn write_remote_event(
    db: &sqlx::SqlitePool,
    state: &AppState,
    calendar_id: &str,
    remote_id: Option<&str>,
    etag: Option<&str>,
    raw_ical: &str,
    req: &NewEvent,
    delete: bool,
) -> Result<(Option<String>, Option<String>), AppError> {
    let row: Option<(
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<Vec<u8>>,
    )> = sqlx::query_as(
        "SELECT c.account_id, c.dav_url, a.type, a.auth_scheme, a.credentials_encrypted \
             FROM calendars c LEFT JOIN calendar_accounts a ON a.id = c.account_id WHERE c.id = ?",
    )
    .bind(calendar_id)
    .fetch_optional(db)
    .await?;
    let Some((account_id, dav_url, provider, auth_scheme, encrypted)) = row else {
        return Err(AppError::NotFound);
    };
    let Some(account_id) = account_id else {
        return Ok((None, None));
    };
    let provider = provider.unwrap_or_default();
    let auth_scheme = auth_scheme.unwrap_or_else(|| "basic".to_owned());
    let encrypted = encrypted.unwrap_or_default();
    let creds = decrypt_calendar_credentials(state, &encrypted).map_err(AppError::BadGateway)?;
    let uid = req.title.as_str();
    match provider.as_str() {
        "caldav" => {
            let auth = dav_auth(&auth_scheme, &creds).map_err(AppError::BadGateway)?;
            if delete {
                if let Some(href) = remote_id {
                    calendar_sync::delete_event(href, &auth, etag)
                        .await
                        .map_err(AppError::BadGateway)?;
                }
                return Ok((None, None));
            }
            let collection =
                dav_url.ok_or_else(|| AppError::BadGateway("calendar dav_url missing".into()))?;
            let resource = remote_id.map(str::to_owned).unwrap_or_else(|| {
                format!(
                    "{}/{}.ics",
                    collection.trim_end_matches('/'),
                    uuid::Uuid::new_v4()
                )
            });
            let new_etag = calendar_sync::put_event(&resource, &auth, raw_ical, etag)
                .await
                .map_err(AppError::BadGateway)?;
            Ok((Some(resource), new_etag))
        }
        "graph" => {
            let token = creds["access_token"]
                .as_str()
                .ok_or_else(|| AppError::BadGateway("access_token missing".into()))?;
            let attendees_json = req.attendees.as_ref().map(Value::to_string);
            let input = event_input(
                uid,
                &req.title,
                &req.starts_at,
                &req.ends_at,
                req.all_day.unwrap_or(false),
                req.location.as_deref(),
                req.description.as_deref(),
                req.rrule.as_deref(),
                attendees_json.as_deref(),
                req.organizer_email.as_deref(),
                req.organizer_name.as_deref(),
            );
            let id = calendar_sync::graph_write(token, remote_id, &input, delete)
                .await
                .map_err(AppError::BadGateway)?;
            Ok((id, None))
        }
        "google" => {
            let token = google_access_token(db, state, &account_id, &creds)
                .await
                .map_err(AppError::BadGateway)?;
            let calendar_remote = dav_url.as_deref().unwrap_or("primary");
            let attendees_json = req.attendees.as_ref().map(Value::to_string);
            let input = event_input(
                uid,
                &req.title,
                &req.starts_at,
                &req.ends_at,
                req.all_day.unwrap_or(false),
                req.location.as_deref(),
                req.description.as_deref(),
                req.rrule.as_deref(),
                attendees_json.as_deref(),
                req.organizer_email.as_deref(),
                req.organizer_name.as_deref(),
            );
            let id =
                calendar_sync::google_write(&token, calendar_remote, remote_id, &input, delete)
                    .await
                    .map_err(AppError::BadGateway)?;
            Ok((id, None))
        }
        _ => Ok((None, None)),
    }
}

fn event_input<'a>(
    uid: &'a str,
    title: &'a str,
    starts_at: &'a str,
    ends_at: &'a str,
    all_day: bool,
    location: Option<&'a str>,
    description: Option<&'a str>,
    rrule: Option<&'a str>,
    attendees_json: Option<&'a str>,
    organizer_email: Option<&'a str>,
    organizer_name: Option<&'a str>,
) -> calendar_sync::EventInput<'a> {
    calendar_sync::EventInput {
        uid,
        title,
        starts_at,
        ends_at,
        all_day,
        location,
        description,
        rrule,
        attendees_json,
        organizer_email,
        organizer_name,
    }
}

fn decrypt_calendar_credentials(state: &AppState, encrypted: &[u8]) -> Result<Value, String> {
    let bytes = state
        .credential_key
        .decrypt(encrypted)
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn dav_auth(auth_scheme: &str, creds: &Value) -> Result<calendar_sync::DavAuth, String> {
    if auth_scheme == "oauth2" {
        return Ok(calendar_sync::DavAuth::Bearer(
            creds["access_token"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        ));
    }
    Ok(calendar_sync::DavAuth::Basic {
        username: creds["username"].as_str().unwrap_or_default().to_owned(),
        password: creds["password"].as_str().unwrap_or_default().to_owned(),
    })
}

async fn discover_caldav_calendars(
    explicit_base: Option<&str>,
    creds: &Value,
    auth: &calendar_sync::DavAuth,
    accept_invalid_tls: bool,
) -> Result<(String, Vec<calendar_sync::DiscoveredCalendar>), String> {
    let mut errors = Vec::new();
    for candidate in caldav_discovery_candidates(explicit_base, creds) {
        match calendar_sync::discover_collections_with_tls_options(
            &candidate,
            auth,
            None,
            accept_invalid_tls,
        )
        .await
        {
            Ok(calendars) if !calendars.is_empty() => return Ok((candidate, calendars)),
            Ok(_) => errors.push(format!("{candidate}: no calendars found")),
            Err(err) => errors.push(format!("{candidate}: {err}")),
        }
    }
    Err(format!(
        "CalDAV discovery failed{}",
        if errors.is_empty() {
            String::new()
        } else {
            format!(" ({})", errors.join("; "))
        }
    ))
}

fn caldav_discovery_candidates(explicit_base: Option<&str>, creds: &Value) -> Vec<String> {
    let username = creds["username"]
        .as_str()
        .or_else(|| creds["email"].as_str())
        .unwrap_or_default()
        .trim()
        .to_owned();
    let domain = username
        .split('@')
        .nth(1)
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let localpart = username.split('@').next().unwrap_or(&username);
    let mut candidates = Vec::new();
    if let Some(base) = explicit_base.map(str::trim).filter(|base| !base.is_empty()) {
        candidates.push(base.to_owned());
    }
    if !domain.is_empty() {
        candidates.extend(provider_caldav_candidates(&domain, &username));
        candidates.extend([
            format!("https://{domain}/.well-known/caldav"),
            format!("https://{domain}/SOGo/dav/{username}/"),
            format!("https://{domain}/SOGo/dav/{username}/Calendar/"),
            format!("https://{domain}/SOGo/dav/{localpart}/"),
            format!("https://{domain}/SOGo/dav/{localpart}/Calendar/"),
            format!("https://mail.{domain}/.well-known/caldav"),
            format!("https://mail.{domain}/SOGo/dav/{username}/"),
            format!("https://mail.{domain}/SOGo/dav/{username}/Calendar/"),
            format!("https://mail.{domain}/SOGo/dav/{localpart}/"),
            format!("https://mail.{domain}/SOGo/dav/{localpart}/Calendar/"),
            format!("https://webmail.{domain}/.well-known/caldav"),
            format!("https://webmail.{domain}/SOGo/dav/{username}/"),
            format!("https://webmail.{domain}/SOGo/dav/{username}/Calendar/"),
            format!("https://webmail.{domain}/SOGo/dav/{localpart}/"),
            format!("https://webmail.{domain}/SOGo/dav/{localpart}/Calendar/"),
            format!("https://sogo.{domain}/.well-known/caldav"),
            format!("https://sogo.{domain}/SOGo/dav/{username}/"),
            format!("https://sogo.{domain}/SOGo/dav/{username}/Calendar/"),
            format!("https://sogo.{domain}/SOGo/dav/{localpart}/"),
            format!("https://sogo.{domain}/SOGo/dav/{localpart}/Calendar/"),
            format!("https://dav.{domain}/.well-known/caldav"),
            format!("https://dav.{domain}/caldav/"),
            format!("https://dav.{domain}/SOGo/dav/{username}/"),
            format!("https://dav.{domain}/SOGo/dav/{username}/Calendar/"),
            format!("https://dav.{domain}/SOGo/dav/{localpart}/"),
            format!("https://dav.{domain}/SOGo/dav/{localpart}/Calendar/"),
            format!("https://caldav.{domain}/"),
            format!("https://calendar.{domain}/.well-known/caldav"),
        ]);
    }
    dedupe_urls(candidates)
}

fn provider_caldav_candidates(domain: &str, username: &str) -> Vec<String> {
    let encoded_user = urlencoding::encode(username);
    match domain {
        "gmail.com" | "googlemail.com" => vec![format!(
            "https://apidata.googleusercontent.com/caldav/v2/{encoded_user}/events/"
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

async fn expand_and_store_rrule(db: &sqlx::SqlitePool, event_id: &str) -> Result<(), AppError> {
    let row: Option<(String, Option<String>, String, String, String, bool, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, String)> =
        sqlx::query_as(
            "SELECT uid, rrule, summary, start_dt, end_dt, all_day, description, location, organizer_email, organizer_name, ms_teams_url, attendees FROM calendar_events WHERE id = ?",
        )
        .bind(event_id)
        .fetch_optional(db)
        .await?;
    let Some((
        uid,
        rrule,
        title,
        starts_at,
        ends_at,
        all_day,
        description,
        location,
        organizer_email,
        organizer_name,
        teams_url,
        attendees,
    )) = row
    else {
        return Ok(());
    };
    if rrule.is_none() {
        return Ok(());
    }
    sqlx::query("DELETE FROM calendar_events WHERE rrule_uid = ? AND id != ?")
        .bind(&uid)
        .bind(event_id)
        .execute(db)
        .await?;
    let master = calendar_sync::ParsedEvent {
        uid: uid.clone(),
        title,
        starts_at: starts_at.clone(),
        ends_at,
        all_day,
        description,
        location,
        rrule,
        rrule_uid: Some(uid.clone()),
        organizer_email,
        organizer_name,
        attendees_json: Some(attendees),
        ms_teams_url: teams_url,
        ..Default::default()
    };
    for occurrence in calendar_sync::expand_rrule(&master, 730)
        .into_iter()
        .skip(1)
    {
        sqlx::query(
            "INSERT INTO calendar_events (calendar_id, uid, summary, start_dt, end_dt, all_day, rrule_uid, recurrence_id, attendees, ms_teams_url) \
             SELECT calendar_id, uid, summary, ?, ?, all_day, ?, ?, attendees, ms_teams_url FROM calendar_events WHERE id = ?",
        )
        .bind(&occurrence.starts_at)
        .bind(&occurrence.ends_at)
        .bind(&uid)
        .bind(&occurrence.recurrence_id)
        .bind(event_id)
        .execute(db)
        .await?;
    }
    Ok(())
}

async fn default_calendar_id(
    state: &AppState,
    user_id: &str,
    user_db: &sqlx::SqlitePool,
) -> Result<Option<String>, AppError> {
    if let Some(id) = sqlx::query_scalar::<_, Option<String>>(
        "SELECT default_calendar_id FROM user_settings WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(&state.app_db)
    .await?
    .flatten()
    {
        return Ok(Some(id));
    }
    sqlx::query_scalar("SELECT id FROM calendars ORDER BY is_default DESC, created_at ASC LIMIT 1")
        .fetch_optional(user_db)
        .await
        .map_err(AppError::from)
}

fn normalize_rsvp(response: &str) -> Result<String, AppError> {
    match response.to_ascii_lowercase().as_str() {
        "accept" | "accepted" => Ok("accepted".to_owned()),
        "tentative" => Ok("tentative".to_owned()),
        "decline" | "declined" => Ok("declined".to_owned()),
        _ => Err(AppError::Unprocessable(
            "response must be accepted, tentative, or declined".into(),
        )),
    }
}

async fn send_rsvp_reply(
    state: &AppState,
    user_db: &sqlx::SqlitePool,
    organizer: &str,
    uid: &str,
    summary: &str,
    start_dt: Option<&str>,
    end_dt: Option<&str>,
    partstat: &str,
) -> Result<(), AppError> {
    let account: Option<(String, String, Vec<u8>, String, i64, String, Option<String>)> = sqlx::query_as(
        "SELECT primary_email, display_name, credentials_encrypted, smtp_host, smtp_port, smtp_auth_scheme, smtp_tls_cert \
         FROM email_accounts WHERE smtp_host IS NOT NULL ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(user_db)
    .await?;
    let Some((from, display_name, encrypted, smtp_host, smtp_port, smtp_auth, smtp_tls_cert)) =
        account
    else {
        return Ok(());
    };
    let creds_bytes = state
        .credential_key
        .decrypt(&encrypted)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let creds: Value =
        serde_json::from_slice(&creds_bytes).map_err(|e| AppError::Internal(e.to_string()))?;
    let smtp_user = creds["smtp_username"].as_str().unwrap_or("").to_owned();
    let smtp_pass = creds["smtp_password"].as_str().unwrap_or("").to_owned();
    let ics = build_rsvp_ics(uid, summary, start_dt, end_dt, organizer, &from, partstat);
    let body = format!("{display_name} responded {partstat} to {summary}.");
    let req = smtp::SendRequest {
        from: from.clone(),
        to: vec![organizer.to_owned()],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: format!("Re: {summary}"),
        body_text: Some(body),
        body_html: None,
        pgp_mime_mode: None,
        pgp_signature: None,
        calendar_method: Some("REPLY".into()),
        calendar_ics: Some(ics),
        in_reply_to: None,
        references: None,
        attachments: Vec::new(),
        smtp_host,
        smtp_port: smtp_port as u16,
        smtp_user,
        smtp_pass,
        oauth_token: creds["oauth_access_token"].as_str().map(str::to_owned),
        auth_scheme: smtp_auth,
        trusted_cert_der: mail_sync::session::decode_trusted_cert(smtp_tls_cert.as_deref()),
    };
    let _ = smtp::send(req)
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?;
    Ok(())
}

fn build_rsvp_ics(
    uid: &str,
    summary: &str,
    start_dt: Option<&str>,
    end_dt: Option<&str>,
    organizer: &str,
    attendee: &str,
    partstat: &str,
) -> String {
    let start = start_dt.unwrap_or("");
    let end = end_dt.unwrap_or(start);
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Mailquill//Calendar//EN\r\nMETHOD:REPLY\r\nBEGIN:VEVENT\r\nUID:{uid}\r\nSUMMARY:{summary}\r\nDTSTART:{}\r\nDTEND:{}\r\nORGANIZER:mailto:{organizer}\r\nATTENDEE;PARTSTAT={partstat}:mailto:{attendee}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        compact_dt(start),
        compact_dt(end)
    )
}

fn compact_dt(value: &str) -> String {
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 14 {
        format!("{}T{}Z", &digits[0..8], &digits[8..14])
    } else {
        digits
    }
}
