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
         WHERE e.starts_at <= ? AND e.ends_at >= ? ORDER BY e.starts_at ASC",
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
    let id: String = sqlx::query_scalar(
        "INSERT INTO calendar_events (calendar_id, title, description, location, starts_at, ends_at, all_day) \
         VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(&req.calendar_id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(&req.location)
    .bind(&req.starts_at)
    .bind(&req.ends_at)
    .bind(req.all_day.unwrap_or(false))
    .fetch_one(&user_db)
    .await?;
    Ok(Json(serde_json::json!({ "id": id })))
}

pub async fn delete_event(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    sqlx::query("DELETE FROM calendar_events WHERE id = ?")
        .bind(&id)
        .execute(&user_db)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}
