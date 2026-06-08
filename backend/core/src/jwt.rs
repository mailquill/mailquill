use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum JwtError {
    #[error("jwt encode error: {0}")]
    Encode(String),
    #[error("jwt decode error: {0}")]
    Decode(String),
    #[error("JWT_SECRET env var required")]
    MissingSecret,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user_id
    pub exp: i64,
    pub iat: i64,
}

pub struct JwtKey {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl JwtKey {
    /// Load JWT secret from JWT_SECRET env var. Panics if absent.
    pub fn from_env() -> Self {
        let secret = std::env::var("JWT_SECRET")
            .unwrap_or_else(|_| panic!("JWT_SECRET env var required"));
        Self::from_secret(secret.as_bytes())
    }

    pub fn from_secret(secret: &[u8]) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
        }
    }

    /// Issue a 15-minute access token for the given user_id.
    pub fn issue_access_token(&self, user_id: &str) -> Result<String, JwtError> {
        let now = Utc::now();
        let claims = Claims {
            sub: user_id.to_owned(),
            iat: now.timestamp(),
            exp: (now + Duration::minutes(15)).timestamp(),
        };
        encode(&Header::default(), &claims, &self.encoding)
            .map_err(|e| JwtError::Encode(e.to_string()))
    }

    /// Validate an access token and return the user_id.
    pub fn validate(&self, token: &str) -> Result<String, JwtError> {
        let data = decode::<Claims>(token, &self.decoding, &Validation::default())
            .map_err(|e| JwtError::Decode(e.to_string()))?;
        Ok(data.claims.sub)
    }
}
