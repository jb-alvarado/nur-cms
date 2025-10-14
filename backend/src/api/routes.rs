use axum::{
    Json,
    extract::{OriginalUri, Query, State},
    http::StatusCode,
};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use sqlx::Postgres;
use tracing::error;

use crate::{
    db::{
        handles,
        models::Role,
        queries::{QueryObj, RespondObj},
    },
    utils::errors::ServiceError,
};

pub async fn welcome(details: AuthDetails<Role>) -> Result<String, (StatusCode, String)> {
    if details.has_authority(&Role::Admin) {
        return Ok("Hello Admin!".to_string());
    } else if details.has_authority(&Role::Author) {
        return Ok("Hello Author!".to_string());
    } else if details.has_authority(&Role::User) {
        return Ok("Hello User!".to_string());
    }

    Ok("Hello Guest!".to_string())
}
