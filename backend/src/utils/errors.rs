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

    #[display("NoContent")]
    NoContent,

    #[display("InvalidInput")]
    InvalidInput,

    #[display("ServiceUnavailable: {_0}")]
    ServiceUnavailable(String),

    #[display("InvalidInput")]
    ToManyRequests,

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
            Self::NoContent => (StatusCode::NO_CONTENT, "No Content".to_string()),
            Self::InvalidInput => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Invalid Input".to_string(),
            ),
            Self::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
            Self::ToManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "Too Many Requests".to_string(),
            ),
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

impl From<markdown::message::Message> for ServiceError {
    fn from(err: markdown::message::Message) -> ServiceError {
        Self::Conflict(err.to_string())
    }
}
impl From<axum::extract::multipart::MultipartError> for ServiceError {
    fn from(err: axum::extract::multipart::MultipartError) -> ServiceError {
        Self::Conflict(err.to_string())
    }
}

impl From<io::Error> for ServiceError {
    fn from(err: io::Error) -> ServiceError {
        error!("{err:?}");
        Self::NoContent
    }
}

impl From<std::string::String> for ServiceError {
    fn from(err: std::string::String) -> ServiceError {
        error!("{err:?}");
        Self::Conflict(err.to_string())
    }
}

impl From<serde_json::Error> for ServiceError {
    fn from(err: serde_json::Error) -> Self {
        error!("{err:?}");
        Self::Conflict(err.to_string())
    }
}

impl From<emval::ValidationError> for ServiceError {
    fn from(err: emval::ValidationError) -> Self {
        error!("{err:?}");
        Self::Conflict("Invalid email address!".to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for ServiceError {
    fn from(_: jsonwebtoken::errors::Error) -> Self {
        Self::Unauthorized
    }
}

impl From<image::ImageError> for ServiceError {
    fn from(err: image::ImageError) -> Self {
        error!("{err:?}");
        Self::InternalServerError
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

impl From<lettre::transport::smtp::Error> for ServiceError {
    fn from(err: lettre::transport::smtp::Error) -> Self {
        error!("{err:?}");
        Self::InternalServerError
    }
}

impl From<lettre::address::AddressError> for ServiceError {
    fn from(err: lettre::address::AddressError) -> Self {
        error!("{err:?}");
        Self::InternalServerError
    }
}

impl From<lettre::error::Error> for ServiceError {
    fn from(err: lettre::error::Error) -> Self {
        error!("{err:?}");
        Self::InternalServerError
    }
}

impl From<sqlx::Error> for ServiceError {
    fn from(err: sqlx::Error) -> ServiceError {
        error!("{err:?}");
        Self::Conflict(err.to_string())
    }
}

impl From<uuid::Error> for ServiceError {
    fn from(err: uuid::Error) -> ServiceError {
        error!("{err:?}");
        Self::InternalServerError
    }
}
