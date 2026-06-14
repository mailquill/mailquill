use axum::{
    extract::{Extension, Path, Query, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{error::AppError, middleware::UserId, state::AppState};

#[derive(Serialize, sqlx::FromRow)]
pub struct Calendar {
    id: String,
    account_id: Option<String>,
    name: String,
    color: String,
}

#[derive(Deserialize)]
pub struct NewCalendar {
    account_id: Option<String>,
    name: String,
    color: Option<String>,
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
}

#[derive(Deserialize)]
pub struct EventRange {
    from: Option<String>,
    to: Option<String>,
}

pub async fn list_calendars(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let calendars: Vec<Calendar> =
        sqlx::query_as("SELECT id, account_id, name, color FROM calendars ORDER BY name COLLATE NOCASE ASC")
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
        "INSERT INTO calendars (account_id, name, color) VALUES (?, ?, ?) RETURNING id",
    )
    .bind(&req.account_id)
    .bind(&req.name)
    .bind(req.color.as_deref().unwrap_or("#2563EB"))
    .fetch_one(&user_db)
    .await?;
    let calendar: Calendar =
        sqlx::query_as("SELECT id, account_id, name, color FROM calendars WHERE id = ?")
            .bind(&id)
            .fetch_one(&user_db)
            .await?;
    Ok(Json(calendar))
}

pub async fn list_events(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(range): Query<EventRange>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let from = range.from.unwrap_or_else(|| "0000".into());
    let to = range.to.unwrap_or_else(|| "9999".into());
    let events: Vec<CalendarEvent> = sqlx::query_as(
        "SELECT e.id, e.calendar_id, e.title, e.description, e.location, e.starts_at, e.ends_at, e.all_day, \
         COALESCE(c.color, '#2563EB') AS color \
         FROM calendar_events e LEFT JOIN calendars c ON c.id = e.calendar_id \
         WHERE e.deleted = 0 AND e.starts_at <= ? AND e.ends_at >= ? ORDER BY e.starts_at ASC",
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
    // Events in a CalDAV-backed calendar are marked dirty so the next sync pushes
    // them; local-only calendars never push.
    let synced = calendar_is_synced(&user_db, &req.calendar_id).await?;
    let uid = format!("{}@mailquill", uuid::Uuid::new_v4());
    let id: String = sqlx::query_scalar(
        "INSERT INTO calendar_events (calendar_id, uid, title, description, location, starts_at, ends_at, all_day, dirty, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now')) RETURNING id",
    )
    .bind(&req.calendar_id)
    .bind(&uid)
    .bind(&req.title)
    .bind(&req.description)
    .bind(&req.location)
    .bind(&req.starts_at)
    .bind(&req.ends_at)
    .bind(req.all_day.unwrap_or(false))
    .bind(synced as i64)
    .fetch_one(&user_db)
    .await?;
    Ok(Json(serde_json::json!({ "id": id })))
}

#[derive(Deserialize)]
pub struct UpdateEvent {
    title: String,
    description: Option<String>,
    location: Option<String>,
    starts_at: String,
    ends_at: String,
    all_day: Option<bool>,
}

pub async fn update_event(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
    Json(req): Json<UpdateEvent>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    // dirty only when the event lives in a CalDAV-backed calendar.
    let synced: bool = sqlx::query_scalar::<_, Option<String>>(
        "SELECT c.dav_url FROM calendar_events e JOIN calendars c ON c.id = e.calendar_id WHERE e.id = ?",
    )
    .bind(&id)
    .fetch_optional(&user_db)
    .await?
    .flatten()
    .is_some();
    let rows = sqlx::query(
        "UPDATE calendar_events SET title=?, description=?, location=?, starts_at=?, ends_at=?, all_day=?, dirty=?, updated_at=datetime('now') WHERE id=?",
    )
    .bind(&req.title)
    .bind(&req.description)
    .bind(&req.location)
    .bind(&req.starts_at)
    .bind(&req.ends_at)
    .bind(req.all_day.unwrap_or(false))
    .bind(synced as i64)
    .bind(&id)
    .execute(&user_db)
    .await?
    .rows_affected();
    if rows == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "id": id })))
}

pub async fn delete_event(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    // If the event has a server resource, tombstone it so the next sync issues a
    // CalDAV DELETE; otherwise remove it outright.
    let href: Option<String> =
        sqlx::query_scalar::<_, Option<String>>("SELECT href FROM calendar_events WHERE id = ?")
            .bind(&id)
            .fetch_optional(&user_db)
            .await?
            .flatten();
    if href.is_some() {
        sqlx::query("UPDATE calendar_events SET deleted = 1, dirty = 0 WHERE id = ?")
            .bind(&id)
            .execute(&user_db)
            .await?;
    } else {
        sqlx::query("DELETE FROM calendar_events WHERE id = ?")
            .bind(&id)
            .execute(&user_db)
            .await?;
    }
    Ok(Json(serde_json::json!({ "deleted": id })))
}

#[derive(Deserialize)]
pub struct UpdateCalendar {
    name: Option<String>,
    color: Option<String>,
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
    let calendar: Calendar =
        sqlx::query_as("SELECT id, account_id, name, color FROM calendars WHERE id = ?")
            .bind(&id)
            .fetch_one(&user_db)
            .await?;
    Ok(Json(calendar))
}

pub async fn delete_calendar(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    sqlx::query("DELETE FROM calendar_events WHERE calendar_id = ?")
        .bind(&id)
        .execute(&user_db)
        .await?;
    sqlx::query("DELETE FROM calendars WHERE id = ?")
        .bind(&id)
        .execute(&user_db)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

/// Whether a calendar is backed by a CalDAV collection (so its events sync).
async fn calendar_is_synced(db: &sqlx::SqlitePool, calendar_id: &str) -> Result<bool, AppError> {
    let dav: Option<String> =
        sqlx::query_scalar::<_, Option<String>>("SELECT dav_url FROM calendars WHERE id = ?")
            .bind(calendar_id)
            .fetch_optional(db)
            .await?
            .flatten();
    Ok(dav.is_some())
}
