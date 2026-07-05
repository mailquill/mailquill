use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::{error::AppError, middleware::UserId, state::AppState, validate};

#[derive(Deserialize)]
pub struct CreatePgpKeyRequest {
    fingerprint: String,
    uid: String,
    public_key_armored: String,
    private_key_encrypted_blob: String,
    is_primary: Option<bool>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct PgpKeyResponse {
    id: String,
    fingerprint: String,
    uid: String,
    public_key_armored: String,
    is_primary: bool,
    created_at: String,
}

#[derive(Serialize)]
pub struct PgpKeyBlobResponse {
    id: String,
    private_key_encrypted_blob: String,
}

#[derive(Deserialize)]
pub struct EmailQuery {
    email: String,
}

#[derive(Deserialize)]
pub struct CreateContactKeyRequest {
    email: String,
    public_key_data: String,
    fingerprint: Option<String>,
}

#[derive(Serialize, sqlx::FromRow, Clone)]
pub struct ContactKeyResponse {
    id: String,
    email: String,
    public_key_data: String,
    source: String,
    fingerprint: String,
    fetched_at: String,
}

#[derive(Serialize)]
pub struct DiscoveryResponse {
    found: bool,
    key: Option<ContactKeyResponse>,
    local: bool,
    wkd_enabled: bool,
    keyserver_enabled: bool,
}

pub async fn create_pgp_key(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<CreatePgpKeyRequest>,
) -> Result<impl IntoResponse, AppError> {
    validate::text("fingerprint", &req.fingerprint, 128)?;
    validate::text("uid", &req.uid, 512)?;
    validate_public_key(&req.public_key_armored)?;
    validate_encrypted_blob(&req.private_key_encrypted_blob)?;

    let user_db = state.user_db_pool.get(&user.0).await?;
    if req.is_primary.unwrap_or(false) {
        sqlx::query("UPDATE pgp_keys SET is_primary = 0")
            .execute(&user_db)
            .await?;
    }

    let id: String = sqlx::query_scalar(
        "INSERT INTO pgp_keys (fingerprint, uid, public_key_armored, private_key_encrypted_blob, is_primary) VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(req.fingerprint.trim().to_uppercase())
    .bind(req.uid.trim())
    .bind(req.public_key_armored.trim())
    .bind(req.private_key_encrypted_blob.as_bytes())
    .bind(req.is_primary.unwrap_or(false))
    .fetch_one(&user_db)
    .await
    .map_err(map_unique)?;

    let row = get_pgp_key_row(&user_db, &id).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn list_pgp_keys(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let keys: Vec<PgpKeyResponse> = sqlx::query_as(
        "SELECT id, fingerprint, uid, public_key_armored, is_primary, created_at FROM pgp_keys ORDER BY is_primary DESC, created_at DESC",
    )
    .fetch_all(&user_db)
    .await?;
    Ok(Json(keys))
}

pub async fn get_pgp_key_blob(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let blob: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT private_key_encrypted_blob FROM pgp_keys WHERE id = ?")
            .bind(&id)
            .fetch_optional(&user_db)
            .await?;
    let blob = blob.ok_or(AppError::NotFound)?;
    let private_key_encrypted_blob = String::from_utf8(blob)
        .map_err(|_| AppError::Internal("PGP key blob is not UTF-8 armor".into()))?;
    Ok(Json(PgpKeyBlobResponse {
        id,
        private_key_encrypted_blob,
    }))
}

pub async fn delete_pgp_key(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let rows = sqlx::query("DELETE FROM pgp_keys WHERE id = ?")
        .bind(&id)
        .execute(&user_db)
        .await?
        .rows_affected();
    if rows == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_primary_pgp_key(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM pgp_keys WHERE id = ?")
        .bind(&id)
        .fetch_optional(&user_db)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }
    sqlx::query("UPDATE pgp_keys SET is_primary = 0")
        .execute(&user_db)
        .await?;
    sqlx::query("UPDATE pgp_keys SET is_primary = 1 WHERE id = ?")
        .bind(&id)
        .execute(&user_db)
        .await?;
    Ok(Json(get_pgp_key_row(&user_db, &id).await?))
}

pub async fn create_contact_key(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Json(req): Json<CreateContactKeyRequest>,
) -> Result<impl IntoResponse, AppError> {
    let email = normalize_email(&req.email)?;
    validate_public_key(&req.public_key_data)?;
    let fingerprint = req
        .fingerprint
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fingerprint_hint(&req.public_key_data));
    validate::text("fingerprint", &fingerprint, 128)?;
    let user_db = state.user_db_pool.get(&user.0).await?;
    let key = upsert_contact_key(
        &user_db,
        &email,
        req.public_key_data.trim(),
        "manual",
        &fingerprint.to_uppercase(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(key)))
}

pub async fn get_contact_key(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(query): Query<EmailQuery>,
) -> Result<impl IntoResponse, AppError> {
    let email = normalize_email(&query.email)?;
    let user_db = state.user_db_pool.get(&user.0).await?;
    let keys = contact_keys_for_email(&user_db, &email).await?;
    Ok(Json(keys))
}

pub async fn discover_key(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(query): Query<EmailQuery>,
) -> Result<impl IntoResponse, AppError> {
    let email = normalize_email(&query.email)?;
    let user_db = state.user_db_pool.get(&user.0).await?;

    if let Some(key) = contact_keys_for_email(&user_db, &email)
        .await?
        .into_iter()
        .next()
    {
        return Ok(Json(DiscoveryResponse {
            found: true,
            key: Some(key),
            local: true,
            wkd_enabled: false,
            keyserver_enabled: false,
        }));
    }

    let (wkd_enabled, keyserver_enabled) = discovery_settings(&state, &user.0).await?;
    if wkd_enabled {
        if let Some(key_data) = fetch_wkd_key(&email).await? {
            let fingerprint = fingerprint_hint(&key_data);
            let key = upsert_contact_key(&user_db, &email, &key_data, "wkd", &fingerprint).await?;
            return Ok(Json(DiscoveryResponse {
                found: true,
                key: Some(key),
                local: false,
                wkd_enabled,
                keyserver_enabled,
            }));
        }
    }
    if keyserver_enabled {
        if let Some(key_data) = fetch_keyserver_key(&email).await? {
            let fingerprint = fingerprint_hint(&key_data);
            let key =
                upsert_contact_key(&user_db, &email, &key_data, "keyserver", &fingerprint).await?;
            return Ok(Json(DiscoveryResponse {
                found: true,
                key: Some(key),
                local: false,
                wkd_enabled,
                keyserver_enabled,
            }));
        }
    }

    Ok(Json(DiscoveryResponse {
        found: false,
        key: None,
        local: false,
        wkd_enabled,
        keyserver_enabled,
    }))
}

async fn get_pgp_key_row(db: &sqlx::SqlitePool, id: &str) -> Result<PgpKeyResponse, AppError> {
    sqlx::query_as(
        "SELECT id, fingerprint, uid, public_key_armored, is_primary, created_at FROM pgp_keys WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound)
}

async fn contact_keys_for_email(
    db: &sqlx::SqlitePool,
    email: &str,
) -> Result<Vec<ContactKeyResponse>, AppError> {
    let rows = sqlx::query_as(
        "SELECT id, email, public_key_data, source, fingerprint, fetched_at FROM contact_keys WHERE email = ? ORDER BY fetched_at DESC",
    )
    .bind(email)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

async fn upsert_contact_key(
    db: &sqlx::SqlitePool,
    email: &str,
    public_key_data: &str,
    source: &str,
    fingerprint: &str,
) -> Result<ContactKeyResponse, AppError> {
    let id: String = sqlx::query_scalar(
        "INSERT INTO contact_keys (email, public_key_data, source, fingerprint) VALUES (?, ?, ?, ?) \
         ON CONFLICT(email, fingerprint) DO UPDATE SET public_key_data=excluded.public_key_data, source=excluded.source, fetched_at=strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
         RETURNING id",
    )
    .bind(email)
    .bind(public_key_data)
    .bind(source)
    .bind(fingerprint)
    .fetch_one(db)
    .await?;
    let row = sqlx::query_as(
        "SELECT id, email, public_key_data, source, fingerprint, fetched_at FROM contact_keys WHERE id = ?",
    )
    .bind(id)
    .fetch_one(db)
    .await?;
    Ok(row)
}

async fn discovery_settings(state: &AppState, user_id: &str) -> Result<(bool, bool), AppError> {
    let row: Option<(bool, bool)> = sqlx::query_as(
        "SELECT pgp_discovery_wkd_enabled, pgp_discovery_keyserver_enabled FROM user_settings WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(&state.app_db)
    .await?;
    Ok(row.unwrap_or((false, false)))
}

async fn fetch_wkd_key(email: &str) -> Result<Option<String>, AppError> {
    let (local, domain) = email
        .split_once('@')
        .ok_or_else(|| AppError::Unprocessable("email must be valid".into()))?;
    let hash = zbase32_sha1(local.as_bytes());
    let url = format!(
        "https://{domain}/.well-known/openpgpkey/hu/{hash}?l={}",
        urlencoding::encode(local)
    );
    fetch_public_key(&url).await
}

async fn fetch_keyserver_key(email: &str) -> Result<Option<String>, AppError> {
    let url = format!(
        "https://keys.openpgp.org/vks/v1/by-email/{}",
        urlencoding::encode(email)
    );
    fetch_public_key(&url).await
}

async fn fetch_public_key(url: &str) -> Result<Option<String>, AppError> {
    let res = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| AppError::Internal(e.to_string()))?
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !res.status().is_success() {
        return Err(AppError::BadGateway(format!(
            "key discovery returned {}",
            res.status()
        )));
    }
    let text = res
        .text()
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?;
    if !looks_like_public_key(&text) {
        return Ok(None);
    }
    Ok(Some(text))
}

fn normalize_email(value: &str) -> Result<String, AppError> {
    let email = value.trim().to_lowercase();
    validate::email("email", &email)?;
    Ok(email)
}

fn validate_public_key(value: &str) -> Result<(), AppError> {
    if !looks_like_public_key(value) {
        return Err(AppError::Unprocessable(
            "public_key_armored must be an armored PGP public key".into(),
        ));
    }
    Ok(())
}

fn validate_encrypted_blob(value: &str) -> Result<(), AppError> {
    let trimmed = value.trim();
    if !trimmed.starts_with("-----BEGIN PGP MESSAGE-----")
        || !trimmed.contains("-----END PGP MESSAGE-----")
    {
        return Err(AppError::Unprocessable(
            "private_key_encrypted_blob must be armored PGP ciphertext".into(),
        ));
    }
    Ok(())
}

fn looks_like_public_key(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----")
        && trimmed.contains("-----END PGP PUBLIC KEY BLOCK-----")
}

fn fingerprint_hint(public_key_data: &str) -> String {
    let digest = sha2::Sha256::digest(public_key_data.as_bytes());
    hex::encode(&digest[..20]).to_uppercase()
}

fn zbase32_sha1(input: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ybndrfg8ejkmcpqxot1uwisza345h769";
    let digest = Sha1::digest(input);
    let mut out = String::new();
    let mut buffer = 0u16;
    let mut bits = 0u8;
    for byte in digest {
        buffer = (buffer << 8) | u16::from(byte);
        bits += 8;
        while bits >= 5 {
            let index = ((buffer >> (bits - 5)) & 0b11111) as usize;
            out.push(ALPHABET[index] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        let index = ((buffer << (5 - bits)) & 0b11111) as usize;
        out.push(ALPHABET[index] as char);
    }
    out
}

fn map_unique(error: sqlx::Error) -> AppError {
    match error {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::Conflict("PGP key fingerprint already exists".into())
        }
        other => AppError::Internal(other.to_string()),
    }
}
