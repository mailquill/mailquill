use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("unprocessable: {0}")]
    Unprocessable(String),
    #[error("bad gateway: {0}")]
    BadGateway(String),
    #[error("bad gateway ({code}): {message}")]
    BadGatewayWithCode { code: &'static str, message: String },
    #[error("internal: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg, code) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string(), None),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string(), None),
            AppError::Forbidden => (StatusCode::FORBIDDEN, self.to_string(), None),
            AppError::Conflict(m) => (StatusCode::CONFLICT, m.clone(), None),
            AppError::Unprocessable(m) => (StatusCode::UNPROCESSABLE_ENTITY, m.clone(), None),
            AppError::BadGateway(m) => (StatusCode::BAD_GATEWAY, m.clone(), None),
            AppError::BadGatewayWithCode { code, message } => {
                (StatusCode::BAD_GATEWAY, message.clone(), Some(*code))
            }
            AppError::Internal(m) => {
                tracing::error!("internal error: {m}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".into(),
                    None,
                )
            }
        };
        let body = match code {
            Some(code) => json!({ "error": msg, "code": code }),
            None => json!({ "error": msg }),
        };
        (status, Json(body)).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

impl From<db::pool::PoolError> for AppError {
    fn from(e: db::pool::PoolError) -> Self {
        AppError::Internal(e.to_string())
    }
}
