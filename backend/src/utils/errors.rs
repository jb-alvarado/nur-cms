use std::io;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use derive_more::Display;
use serde_json::json;
use tracing::error;

#[derive(Debug, Display)]
pub enum ServiceError {
    #[display("Internal Server Error")]
    InternalServerError,

    #[display("BadRequest: {_0}")]
    BadRequest(String),

    #[display("Conflict: {_0}")]
    Conflict(String),

    #[display("Forbidden: {_0}")]
    Forbidden(String),

    #[display("Unauthorized")]
    Unauthorized,

    #[display("NoContent: {_0}")]
    NoContent(String),

    #[display("ServiceUnavailable: {_0}")]
    ServiceUnavailable(String),

    #[display("UnprocessableEntity: {_0}")]
    UnprocessableEntity(String),
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::InternalServerError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error. Please try again later.".to_string(),
            ),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::Conflict(msg) => (StatusCode::CONFLICT, msg),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            Self::NoContent(msg) => (StatusCode::NO_CONTENT, msg),
            Self::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
            Self::UnprocessableEntity(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg),
        };

        if status == StatusCode::NO_CONTENT {
            return ().into_response();
        }

        let body = Json(json!({ "error": message }));
        (status, body).into_response()
    }
}

impl From<ServiceError> for io::Error {
    fn from(err: ServiceError) -> Self {
        io::Error::other(format!("{err:?}"))
    }
}

impl From<inquire::InquireError> for ServiceError {
    fn from(err: inquire::InquireError) -> ServiceError {
        Self::Conflict(err.to_string())
    }
}

impl From<io::Error> for ServiceError {
    fn from(err: io::Error) -> ServiceError {
        Self::NoContent(err.to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for ServiceError {
    fn from(_: jsonwebtoken::errors::Error) -> Self {
        Self::Unauthorized
    }
}

impl From<tokio::task::JoinError> for ServiceError {
    fn from(err: tokio::task::JoinError) -> Self {
        error!("{err:?}");
        Self::InternalServerError
    }
}

impl From<sqlx::migrate::MigrateError> for ServiceError {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        error!("{err:?}");
        Self::InternalServerError
    }
}

impl From<sqlx::Error> for ServiceError {
    fn from(err: sqlx::Error) -> ServiceError {
        error!("{err:?}");
        Self::InternalServerError
    }
}
