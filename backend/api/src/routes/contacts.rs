use axum::{
    extract::{Extension, Path, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{error::AppError, middleware::UserId, state::AppState};

#[derive(Serialize, sqlx::FromRow)]
pub struct Contact {
    id: String,
    account_id: Option<String>,
    display_name: String,
    email: Option<String>,
    phone: Option<String>,
    company: Option<String>,
    job_title: Option<String>,
    notes: Option<String>,
    favorite: bool,
    group_name: Option<String>,
}

#[derive(Deserialize)]
pub struct NewContact {
    account_id: Option<String>,
    display_name: String,
    email: Option<String>,
    phone: Option<String>,
    company: Option<String>,
    job_title: Option<String>,
    notes: Option<String>,
    favorite: Option<bool>,
    group_name: Option<String>,
}

const SELECT_COLS: &str =
    "id, account_id, display_name, email, phone, company, job_title, notes, favorite, group_name";

pub async fn list_contacts(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let contacts: Vec<Contact> = sqlx::query_as(&format!(
        "SELECT {SELECT_COLS} FROM contacts ORDER BY display_name COLLATE NOCASE ASC"
    ))
    .fetch_all(&user_db)
    .await?;
    Ok(Json(contacts))
}

pub async fn create_contact(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<NewContact>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let id: String = sqlx::query_scalar(
        "INSERT INTO contacts (account_id, display_name, email, phone, company, job_title, notes, favorite, group_name) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(&req.account_id)
    .bind(&req.display_name)
    .bind(&req.email)
    .bind(&req.phone)
    .bind(&req.company)
    .bind(&req.job_title)
    .bind(&req.notes)
    .bind(req.favorite.unwrap_or(false))
    .bind(&req.group_name)
    .fetch_one(&user_db)
    .await?;

    let contact: Contact =
        sqlx::query_as(&format!("SELECT {SELECT_COLS} FROM contacts WHERE id = ?"))
            .bind(&id)
            .fetch_one(&user_db)
            .await?;
    Ok(Json(contact))
}

pub async fn delete_contact(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    sqlx::query("DELETE FROM contacts WHERE id = ?")
        .bind(&id)
        .execute(&user_db)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}
