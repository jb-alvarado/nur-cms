use argon2::{Argon2, PasswordVerifier, password_hash::PasswordHash};
use axum::{Json as AxumJson, extract::State, http::StatusCode, response::IntoResponse};
use chrono::{TimeDelta, Utc};
use jsonwebtoken::{self, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use tokio::task;
use tracing::{error, info};

use crate::{
    ACCESS_LIFETIME, JWT_SECRET, REFRESH_LIFETIME,
    db::{
        handles,
        models::{AuthUser, Role},
    },
    utils::errors::ServiceError,
};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Claims {
    pub id: i32,
    pub username: String,
    pub role: Role,
    exp: i64,
}

impl Claims {
    pub fn new(user: AuthUser, role: Role, lifetime: i64) -> Self {
        Self {
            id: user.id,
            username: user.username,
            role,
            exp: (Utc::now() + TimeDelta::try_days(lifetime).unwrap()).timestamp(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TokenRefreshRequest {
    pub refresh: String,
}

/// Create a json web token (JWT)
pub async fn encode_jwt(claims: Claims) -> Result<String, ServiceError> {
    let encoding_key = EncodingKey::from_secret(JWT_SECRET.as_bytes());
    Ok(jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &encoding_key,
    )?)
}

/// Decode a json web token (JWT)
pub async fn decode_jwt(token: &str) -> Result<Claims, ServiceError> {
    let decoding_key = DecodingKey::from_secret(JWT_SECRET.as_bytes());
    jsonwebtoken::decode::<Claims>(token, &decoding_key, &Validation::default())
        .map(|data| data.claims)
        .map_err(|_| ServiceError::Unauthorized)
}

pub async fn login(
    State(pool): State<Pool<Postgres>>,
    AxumJson(credentials): AxumJson<Credentials>,
) -> Result<impl IntoResponse, ServiceError> {
    let username = credentials.username.clone();
    let password = credentials.password.clone();

    match handles::select_auth_user(&pool, None, Some(&username)).await {
        Ok(mut user) => {
            if user.username.is_empty() {
                return Ok((
                    StatusCode::FORBIDDEN,
                    AxumJson(serde_json::json!({
                        "detail": "Incorrect credentials!",
                    })),
                ));
            }

            let role = handles::select_auth_role(&pool, Some(&user.role_id)).await?[0].clone();
            let pass_hash = user.password.clone();
            let cred_password = password.clone();

            user.password = String::new();

            let verified_password = task::spawn_blocking(move || {
                let hash = PasswordHash::new(&pass_hash)?;
                Argon2::default().verify_password(cred_password.as_bytes(), &hash)
            })
            .await?;

            if verified_password.is_ok() {
                let user_id = user.id;

                let access_claims = Claims::new(user.clone(), role.name.clone(), ACCESS_LIFETIME);
                let access_token = encode_jwt(access_claims).await?;
                let refresh_claims = Claims::new(user, role.name.clone(), REFRESH_LIFETIME);
                let refresh_token = encode_jwt(refresh_claims).await?;

                handles::update_auth_user_last_login(&pool, user_id).await?;

                tracing::info!("user {username} login, with role: {}", role.name);

                return Ok((
                    StatusCode::OK,
                    AxumJson(serde_json::json!({
                        "access": access_token,
                        "refresh": refresh_token,
                    })),
                ));
            }

            error!("Wrong password for {username}!");

            Ok((
                StatusCode::BAD_REQUEST,
                AxumJson(serde_json::json!({
                    "detail": "Incorrect credentials!",
                })),
            ))
        }
        Err(e) => {
            error!("Login {username} failed! {e}");

            Ok((
                StatusCode::BAD_REQUEST,
                AxumJson(serde_json::json!({
                    "detail": format!("Login {username} failed!"),
                })),
            ))
        }
    }
}

pub async fn refresh(
    State(pool): State<Pool<Postgres>>,
    AxumJson(data): AxumJson<TokenRefreshRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    let refresh_token = &data.refresh;

    match decode_jwt(refresh_token).await {
        Ok(claims) => {
            let user_id = claims.id;
            let role = claims.role;

            if let Ok(user) = handles::select_auth_user(&pool, Some(user_id), None).await {
                let access_claims = Claims::new(user.clone(), role.clone(), ACCESS_LIFETIME);
                let access_token = encode_jwt(access_claims).await?;

                info!("user {} refresh, with role: {role}", user.username);

                return Ok((
                    StatusCode::OK,
                    AxumJson(serde_json::json!({
                        "access": access_token,
                    })),
                ));
            }

            Ok((
                StatusCode::UNAUTHORIZED,
                AxumJson(serde_json::json!({
                    "detail": "Invalid user in refresh token",
                })),
            ))
        }
        Err(_) => Ok((
            StatusCode::FORBIDDEN,
            AxumJson(serde_json::json!({
                "detail": "Invalid refresh token",
            })),
        )),
    }
}
