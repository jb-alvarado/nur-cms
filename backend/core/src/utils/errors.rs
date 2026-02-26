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
pub enum NurError {
    // 500 Internal Server Error
    #[display("Internal Server Error")]
    InternalServerError,

    // 400 Bad Request
    #[display("BadRequest: {_0}")]
    BadRequest(String),

    // 409 Conflict
    #[display("Conflict: {_0}")]
    Conflict(String),

    // 403 Forbidden
    #[display("Forbidden: {_0}")]
    Forbidden(String),

    // 401 Unauthorized
    #[display("Unauthorized")]
    Unauthorized,

    // 204 No Content
    #[display("NoContent")]
    NoContent,

    // 422 Unprocessable Entity
    #[display("InvalidInput")]
    InvalidInput,

    // 503 Service Unavailable
    #[display("ServiceUnavailable: {_0}")]
    ServiceUnavailable(String),

    // 429 Too Many Requests
    #[display("InvalidInput")]
    ToManyRequests,

    // 422 Unprocessable Entity
    #[display("UnprocessableEntity: {_0}")]
    UnprocessableEntity(String),
}

impl IntoResponse for NurError {
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

impl From<NurError> for io::Error {
    fn from(err: NurError) -> Self {
        io::Error::other(format!("{err:?}"))
    }
}

impl From<inquire::InquireError> for NurError {
    fn from(err: inquire::InquireError) -> NurError {
        Self::Conflict(err.to_string())
    }
}

impl From<markdown::message::Message> for NurError {
    fn from(err: markdown::message::Message) -> NurError {
        Self::Conflict(err.to_string())
    }
}
impl From<axum::extract::multipart::MultipartError> for NurError {
    fn from(err: axum::extract::multipart::MultipartError) -> NurError {
        Self::Conflict(err.to_string())
    }
}

impl From<io::Error> for NurError {
    fn from(err: io::Error) -> NurError {
        error!("{err:?}");
        Self::NoContent
    }
}

impl From<std::string::String> for NurError {
    fn from(err: std::string::String) -> NurError {
        error!("{err:?}");
        Self::Conflict(err.to_string())
    }
}

impl From<serde_json::Error> for NurError {
    fn from(err: serde_json::Error) -> Self {
        error!("{err:?}");
        Self::Conflict(err.to_string())
    }
}

impl From<emval::ValidationError> for NurError {
    fn from(err: emval::ValidationError) -> Self {
        error!("{err:?}");
        Self::Conflict("Invalid email address!".to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for NurError {
    fn from(_: jsonwebtoken::errors::Error) -> Self {
        Self::Unauthorized
    }
}

impl From<image::ImageError> for NurError {
    fn from(err: image::ImageError) -> Self {
        error!("{err:?}");
        Self::InternalServerError
    }
}

impl From<tokio::task::JoinError> for NurError {
    fn from(err: tokio::task::JoinError) -> Self {
        error!("{err:?}");
        Self::InternalServerError
    }
}

impl From<sqlx::migrate::MigrateError> for NurError {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        error!("{err:?}");
        Self::InternalServerError
    }
}

impl From<lettre::transport::smtp::Error> for NurError {
    fn from(err: lettre::transport::smtp::Error) -> Self {
        error!("{err:?}");
        Self::InternalServerError
    }
}

impl From<lettre::address::AddressError> for NurError {
    fn from(err: lettre::address::AddressError) -> Self {
        error!("{err:?}");
        Self::InternalServerError
    }
}

impl From<lettre::error::Error> for NurError {
    fn from(err: lettre::error::Error) -> Self {
        error!("{err:?}");
        Self::InternalServerError
    }
}

impl From<sqlx::Error> for NurError {
    fn from(err: sqlx::Error) -> NurError {
        error!("{err:?}");
        Self::Conflict(err.to_string())
    }
}

impl From<uuid::Error> for NurError {
    fn from(err: uuid::Error) -> NurError {
        error!("{err:?}");
        Self::InternalServerError
    }
}
