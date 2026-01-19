use argon2::{Argon2, PasswordVerifier, password_hash::PasswordHash};
use axum::{Json as AxumJson, extract::State, http::StatusCode, response::IntoResponse};
use chrono::{DateTime, Local, TimeDelta, Utc};
use jsonwebtoken::{self, DecodingKey, EncodingKey, Header, Validation};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;
use tokio::task;
use tracing::{error, info, warn};

use crate::{
    ACCESS_LIFETIME, CONFIG, REFRESH_LIFETIME,
    db::{
        fields::{AuthUserFields, Table},
        handles,
        models::{AuthUser, MailTarget, Role},
        queries::QueryObj,
    },
    mail::client::{Msg, message},
    utils::errors::ServiceError,
};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Claims {
    pub id: i32,
    pub role: Role,
    exp: i64,
}

impl Claims {
    pub fn new(id: i32, role: Role, lifetime: i64) -> Self {
        Self {
            id,
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

#[derive(Clone, Debug)]
pub struct VerificationCode {
    pub code: String,
    pub user_id: i32,
    pub role: Role,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VerifyRequest {
    pub username: String,
    pub code: String,
}

// Global storage for verification codes
pub static VERIFICATION_CODES: LazyLock<Arc<Mutex<HashMap<String, VerificationCode>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

/// Create a json web token (JWT)
pub async fn encode_jwt(claims: Claims) -> Result<String, ServiceError> {
    let encoding_key = EncodingKey::from_secret(CONFIG.read().await.jwt_secret.as_bytes());
    Ok(jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &encoding_key,
    )?)
}

/// Decode a json web token (JWT)
pub async fn decode_jwt(token: &str) -> Result<Claims, ServiceError> {
    let decoding_key = DecodingKey::from_secret(CONFIG.read().await.jwt_secret.as_bytes());
    jsonwebtoken::decode::<Claims>(token, &decoding_key, &Validation::default())
        .map(|data| data.claims)
        .map_err(|_| ServiceError::Unauthorized)
}

pub async fn verify(
    State(pool): State<PgPool>,
    AxumJson(request): AxumJson<VerifyRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    let username = request.username;
    let provided_code = request.code;

    // Check if code exists
    let verification_data = {
        let mut codes = VERIFICATION_CODES.lock().await;

        if let Some(verification) = codes.get(&username) {
            // Check if code is still valid (max 5 minutes)
            let elapsed = Utc::now().signed_duration_since(verification.created_at);
            if elapsed.num_minutes() > 5 {
                codes.remove(&username);
                return Ok((
                    StatusCode::BAD_REQUEST,
                    AxumJson(serde_json::json!({
                        "detail": "Verification code expired!",
                    })),
                )
                    .into_response());
            }

            // Check if code is correct
            if verification.code != provided_code {
                return Ok((
                    StatusCode::FORBIDDEN,
                    AxumJson(serde_json::json!({
                        "detail": "Invalid verification code!",
                    })),
                )
                    .into_response());
            }

            // Code is valid, remove it and return data
            let data = verification.clone();
            codes.remove(&username);
            Some(data)
        } else {
            None
        }
    };

    match verification_data {
        Some(verification) => {
            let user_id = verification.user_id;
            let role = verification.role;

            // Generate JWT tokens
            let access_claims = Claims::new(user_id, role.clone(), *ACCESS_LIFETIME);
            let access_token = encode_jwt(access_claims).await?;
            let refresh_claims = Claims::new(user_id, role.clone(), *REFRESH_LIFETIME);
            let refresh_token = encode_jwt(refresh_claims).await?;

            // Update last_login
            let auth_user = AuthUser {
                last_login: Some(Local::now().into()),
                ..Default::default()
            };
            handles::update_record(&pool, &Table::AuthUsers, user_id, &auth_user).await?;

            info!("User {username} verified successfully, with role: {}", role);

            Ok((
                StatusCode::OK,
                AxumJson(serde_json::json!({
                    "access": access_token,
                    "refresh": refresh_token,
                })),
            )
                .into_response())
        }
        None => {
            error!("No verification code found for {username}");
            Ok((
                StatusCode::FORBIDDEN,
                AxumJson(serde_json::json!({
                    "detail": "No verification code found or code expired!",
                })),
            )
                .into_response())
        }
    }
}

pub async fn login(
    State(pool): State<PgPool>,
    AxumJson(credentials): AxumJson<Credentials>,
) -> Result<impl IntoResponse, ServiceError> {
    let username = credentials.username.clone();
    let password = credentials.password.clone();
    let query_obj: QueryObj<AuthUserFields> = QueryObj {
        fields: vec![
            AuthUserFields::ID,
            AuthUserFields::Username,
            AuthUserFields::Email,
            AuthUserFields::Password,
            AuthUserFields::Role,
        ],
        search: Some(username.clone()),
        ..Default::default()
    };

    match handles::select_auth_user(&pool, query_obj).await {
        Ok(resp) => {
            if resp.results.is_empty() {
                return Ok((
                    StatusCode::FORBIDDEN,
                    AxumJson(serde_json::json!({
                        "detail": "Incorrect credentials!",
                    })),
                )
                    .into_response());
            }

            let mut user = resp.results[0].clone();
            let role = user.role.clone().unwrap();

            let pass_hash = user.password.unwrap_or_default().clone();
            let cred_password = password.clone();

            user.password = None;

            let verified_password = task::spawn_blocking(move || {
                let hash = PasswordHash::new(&pass_hash)?;
                Argon2::default().verify_password(cred_password.as_bytes(), &hash)
            })
            .await?;

            if verified_password.is_ok() {
                let config = CONFIG.read().await.clone();

                if let Some(email) = user.email.clone()
                    && config.mail_user.is_some()
                    && config.mail_password.is_some()
                    && config.mail_smtp.is_some()
                {
                    // Generate 7-digit random code
                    let verification_code: String = (0..7)
                        .map(|_| rand::rng().random_range(0..10).to_string())
                        .collect();

                    // Store code with timestamp
                    let user_id = user.id.unwrap();
                    let verification_entry = VerificationCode {
                        code: verification_code.clone(),
                        user_id,
                        role: role.name.clone(),
                        created_at: Utc::now(),
                    };

                    VERIFICATION_CODES
                        .lock()
                        .await
                        .insert(username.clone(), verification_entry);

                    // Start cleanup task for this code
                    let username_cleanup = username.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(300)).await; // 5 minutes
                        VERIFICATION_CODES.lock().await.remove(&username_cleanup);
                        info!("Verification code for {username_cleanup} expired and removed");
                    });

                    let text = mail_body(&verification_code);

                    let target = MailTarget::new(email, true);
                    let msg = Msg::new(
                        config.mail_user.unwrap(),
                        "NUR CMS".to_string(),
                        Some(format!("Your NUR CMS code is: {verification_code}")),
                        text,
                        target,
                    );

                    message(msg).await?;

                    info!("Send verification code");

                    return Ok((
                        StatusCode::OK,
                        AxumJson(serde_json::json!({
                            "detail": "Verification code sended to email!",
                        })),
                    )
                        .into_response());
                }

                warn!("Two-factor authentication is not possible!");

                let user_id = user.id.unwrap();

                let access_claims = Claims::new(user_id, role.name.clone(), *ACCESS_LIFETIME);
                let access_token = encode_jwt(access_claims).await?;
                let refresh_claims = Claims::new(user_id, role.name.clone(), *REFRESH_LIFETIME);
                let refresh_token = encode_jwt(refresh_claims).await?;
                let auth_user = AuthUser {
                    last_login: Some(Local::now().into()),
                    ..Default::default()
                };

                handles::update_record(&pool, &Table::AuthUsers, user_id, &auth_user).await?;

                info!("user {username} login, with role: {}", role.name);

                return Ok((
                    StatusCode::OK,
                    AxumJson(serde_json::json!({
                        "access": access_token,
                        "refresh": refresh_token,
                    })),
                )
                    .into_response());
            }

            error!("Wrong password for {username}!");

            Ok((
                StatusCode::FORBIDDEN,
                AxumJson(serde_json::json!({
                    "detail": "Incorrect credentials!",
                })),
            )
                .into_response())
        }
        Err(e) => {
            error!("Login {username} failed! {e}");

            Ok((
                StatusCode::BAD_REQUEST,
                AxumJson(serde_json::json!({
                    "detail": format!("Login {username} failed!"),
                })),
            )
                .into_response())
        }
    }
}

pub async fn refresh(
    State(pool): State<PgPool>,
    AxumJson(data): AxumJson<TokenRefreshRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    let refresh_token = &data.refresh;

    match decode_jwt(refresh_token).await {
        Ok(claims) => {
            let user_id = claims.id;
            let claim_role = claims.role;

            let query_obj: QueryObj<AuthUserFields> = QueryObj {
                fields: vec![
                    AuthUserFields::ID,
                    AuthUserFields::Username,
                    AuthUserFields::Role,
                ],
                search_id: Some(user_id),
                ..Default::default()
            };

            if let Ok(resp) = handles::select_auth_user(&pool, query_obj).await
                && !resp.results.is_empty()
            {
                let username = resp.results[0].username.clone();
                let role_name = resp.results[0]
                    .role
                    .clone()
                    .ok_or(ServiceError::Unauthorized)?
                    .name;

                if role_name != claim_role {
                    return Ok((
                        StatusCode::UNAUTHORIZED,
                        AxumJson(serde_json::json!({
                            "detail": "Role mismatch in refresh token",
                        })),
                    ));
                }

                let access_claims = Claims::new(user_id, role_name.clone(), *ACCESS_LIFETIME);
                let access_token = encode_jwt(access_claims).await?;

                info!("user {} refresh, with role: {role_name}", username.unwrap());

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

fn mail_body(verification_code: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
    <html lang="en">
    <head>
        <meta charset="UTF-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    </head>
    <body>
        <div style="padding: 2px 15px;">
            <div>
                <h2>Your verification code</h2>
                <p>Enter this code in the <b>NUR CMS</b> verification step to finish signing in:</p>
                <p style="padding: 5px; font-size: 20px; font-weight: bold;">{verification_code}</p>
                <p>This code expires in 5 minutes. If you did not request it, you can ignore this email.</p>
                <div>
                    This message was sent automatically by <b>NUR CMS</b>.
                </div>
            </div>
        </div>
    </body>
    </html>"#
    )
}
