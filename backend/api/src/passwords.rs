use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use rand::rngs::OsRng;
use sqlx::SqlitePool;

pub const MIN_PASSWORD_LENGTH: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum PasswordResetError {
    #[error("password must be at least {MIN_PASSWORD_LENGTH} characters")]
    TooShort,
    #[error("account not found")]
    AccountNotFound,
    #[error("failed to hash password: {0}")]
    Hash(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Hash a local-account password using the application's Argon2 parameters.
///
/// # Errors
/// Returns an error when the password violates policy or Argon2 hashing fails.
pub fn hash_password(password: &str) -> Result<String, PasswordResetError> {
    if password.len() < MIN_PASSWORD_LENGTH {
        return Err(PasswordResetError::TooShort);
    }

    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| PasswordResetError::Hash(error.to_string()))
}

/// Replace a local account password and revoke all of its refresh sessions.
///
/// # Errors
/// Returns an error when the account does not exist, the password violates
/// policy, hashing fails, or the database update cannot be committed.
pub async fn reset_password(
    db: &SqlitePool,
    email: &str,
    password: &str,
) -> Result<(), PasswordResetError> {
    let email = email.trim().to_lowercase();
    let hash = hash_password(password)?;
    let mut transaction = db.begin().await?;
    let user_id: Option<String> = sqlx::query_scalar("SELECT id FROM users WHERE email = ?")
        .bind(&email)
        .fetch_optional(&mut *transaction)
        .await?;
    let user_id = user_id.ok_or(PasswordResetError::AccountNotFound)?;

    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(hash)
        .bind(&user_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("UPDATE refresh_tokens SET revoked = 1 WHERE user_id = ?")
        .bind(&user_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{reset_password, PasswordResetError};
    use argon2::{password_hash::PasswordHash, PasswordVerifier};
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_db() -> sqlx::SqlitePool {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(
            "CREATE TABLE users (
                id TEXT PRIMARY KEY,
                email TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL
             );
             CREATE TABLE refresh_tokens (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                revoked INTEGER NOT NULL DEFAULT 0
             );
             INSERT INTO users VALUES ('user-1', 'user@example.com', 'old-hash');
             INSERT INTO refresh_tokens VALUES ('token-1', 'user-1', 0);",
        )
        .execute(&db)
        .await
        .unwrap();
        db
    }

    #[tokio::test]
    async fn reset_hashes_password_case_insensitively_and_revokes_sessions() {
        let db = test_db().await;

        reset_password(&db, " User@Example.COM ", "new-password")
            .await
            .unwrap();

        let hash: String = sqlx::query_scalar("SELECT password_hash FROM users")
            .fetch_one(&db)
            .await
            .unwrap();
        let parsed = PasswordHash::new(&hash).unwrap();
        assert!(argon2::Argon2::default()
            .verify_password(b"new-password", &parsed)
            .is_ok());
        let revoked: bool = sqlx::query_scalar("SELECT revoked FROM refresh_tokens")
            .fetch_one(&db)
            .await
            .unwrap();
        assert!(revoked);
    }

    #[tokio::test]
    async fn reset_rejects_short_passwords_and_unknown_accounts() {
        let db = test_db().await;

        assert!(matches!(
            reset_password(&db, "user@example.com", "short").await,
            Err(PasswordResetError::TooShort)
        ));
        assert!(matches!(
            reset_password(&db, "missing@example.com", "new-password").await,
            Err(PasswordResetError::AccountNotFound)
        ));
    }
}
