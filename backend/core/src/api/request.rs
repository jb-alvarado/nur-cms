use axum::{
    extract::{FromRequest, FromRequestParts, Json, Query, Request},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;
use tracing::warn;

pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Query::<T>::from_request_parts(parts, state).await {
            Ok(Query(value)) => Ok(ApiQuery(value)),
            Err(err) => {
                warn!(error = %err, "invalid query params");

                // Dev vs. Prod Verhalten
                let message = if cfg!(debug_assertions) {
                    err.to_string()
                } else {
                    "Invalid Request".to_string()
                };

                Err((
                    StatusCode::BAD_REQUEST,
                    axum::Json(serde_json::json!({
                        "error": message
                    })),
                )
                    .into_response())
            }
        }
    }
}

pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(ApiJson(value)),
            Err(err) => {
                warn!(error = %err, "invalid json payload");

                // Dev vs. Prod Verhalten
                let message = if cfg!(debug_assertions) {
                    err.to_string()
                } else {
                    "Invalid Request".to_string()
                };

                Err((
                    StatusCode::BAD_REQUEST,
                    axum::Json(serde_json::json!({
                        "error": message
                    })),
                )
                    .into_response())
            }
        }
    }
}
