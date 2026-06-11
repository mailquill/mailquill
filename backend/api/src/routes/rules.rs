use axum::{
    extract::{Extension, Path, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{error::AppError, middleware::UserId, state::AppState};

#[derive(sqlx::FromRow)]
struct RuleRow {
    id: String,
    account_id: Option<String>,
    name: String,
    enabled: bool,
    engine: String,
    match_all: bool,
    conditions: String,
    actions: String,
}

#[derive(Serialize)]
pub struct Rule {
    id: String,
    account_id: Option<String>,
    name: String,
    enabled: bool,
    engine: String,
    match_all: bool,
    conditions: Value,
    actions: Value,
}

impl From<RuleRow> for Rule {
    fn from(r: RuleRow) -> Self {
        Rule {
            id: r.id,
            account_id: r.account_id,
            name: r.name,
            enabled: r.enabled,
            engine: r.engine,
            match_all: r.match_all,
            conditions: serde_json::from_str(&r.conditions).unwrap_or(Value::Array(vec![])),
            actions: serde_json::from_str(&r.actions).unwrap_or(Value::Array(vec![])),
        }
    }
}

#[derive(Deserialize)]
pub struct RuleInput {
    account_id: Option<String>,
    name: String,
    enabled: Option<bool>,
    engine: Option<String>,
    match_all: Option<bool>,
    conditions: Option<Value>,
    actions: Option<Value>,
}

const COLS: &str = "id, account_id, name, enabled, engine, match_all, conditions, actions";

pub async fn list_rules(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let rows: Vec<RuleRow> = sqlx::query_as(&format!("SELECT {COLS} FROM inbox_rules ORDER BY created_at DESC"))
        .fetch_all(&user_db)
        .await?;
    Ok(Json(rows.into_iter().map(Rule::from).collect::<Vec<_>>()))
}

pub async fn create_rule(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<RuleInput>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let id: String = sqlx::query_scalar(
        "INSERT INTO inbox_rules (account_id, name, enabled, engine, match_all, conditions, actions) \
         VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(&req.account_id)
    .bind(&req.name)
    .bind(req.enabled.unwrap_or(true))
    .bind(req.engine.as_deref().unwrap_or("sieve"))
    .bind(req.match_all.unwrap_or(true))
    .bind(json_text(&req.conditions))
    .bind(json_text(&req.actions))
    .fetch_one(&user_db)
    .await?;
    fetch_one(&user_db, &id).await
}

pub async fn update_rule(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
    Json(req): Json<RuleInput>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    sqlx::query(
        "UPDATE inbox_rules SET account_id = ?, name = ?, enabled = ?, engine = ?, match_all = ?, conditions = ?, actions = ? WHERE id = ?",
    )
    .bind(&req.account_id)
    .bind(&req.name)
    .bind(req.enabled.unwrap_or(true))
    .bind(req.engine.as_deref().unwrap_or("sieve"))
    .bind(req.match_all.unwrap_or(true))
    .bind(json_text(&req.conditions))
    .bind(json_text(&req.actions))
    .bind(&id)
    .execute(&user_db)
    .await?;
    fetch_one(&user_db, &id).await
}

pub async fn delete_rule(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    sqlx::query("DELETE FROM inbox_rules WHERE id = ?")
        .bind(&id)
        .execute(&user_db)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

fn json_text(value: &Option<Value>) -> String {
    value
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "[]".to_owned())
}

async fn fetch_one(db: &sqlx::SqlitePool, id: &str) -> Result<Json<Rule>, AppError> {
    let row: RuleRow = sqlx::query_as(&format!("SELECT {COLS} FROM inbox_rules WHERE id = ?"))
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(Json(Rule::from(row)))
}

/// Compile every enabled Sieve rule for an account and upload it as the active
/// ManageSieve script. Only valid for IMAP/Sieve (basic-auth) accounts.
pub async fn apply_sieve(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(account_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;

    let row: Option<(String, Vec<u8>)> =
        sqlx::query_as("SELECT imap_host, credentials_encrypted FROM email_accounts WHERE id = ?")
            .bind(&account_id)
            .fetch_optional(&user_db)
            .await?;
    let (imap_host, enc) = row.ok_or(AppError::NotFound)?;

    let creds: Value = serde_json::from_slice(
        &state.credential_key.decrypt(&enc).map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .map_err(|e| AppError::Internal(e.to_string()))?;
    if creds["oauth_access_token"].as_str().is_some() {
        return Err(AppError::Unprocessable(
            "Sieve upload is not supported for OAuth accounts".into(),
        ));
    }
    let username = creds["imap_username"].as_str().unwrap_or("").to_owned();
    let password = creds["imap_password"].as_str().unwrap_or("").to_owned();

    let rows: Vec<(String, bool, String, String)> = sqlx::query_as(
        "SELECT name, match_all, conditions, actions FROM inbox_rules \
         WHERE enabled = 1 AND engine = 'sieve' AND (account_id = ? OR account_id IS NULL)",
    )
    .bind(&account_id)
    .fetch_all(&user_db)
    .await?;

    let compiled: Vec<crate::sieve::CompiledRule> = rows
        .into_iter()
        .map(|(name, match_all, conditions, actions)| crate::sieve::CompiledRule {
            name,
            match_all,
            conditions: serde_json::from_str(&conditions).unwrap_or_default(),
            actions: serde_json::from_str(&actions).unwrap_or_default(),
        })
        .collect();

    let script = crate::sieve::compile_sieve(&compiled);
    crate::sieve::upload_script(&imap_host, 4190, &username, &password, "mailtastic", &script)
        .await
        .map_err(AppError::BadGateway)?;

    Ok(Json(serde_json::json!({ "uploaded": true, "rules": compiled.len() })))
}
