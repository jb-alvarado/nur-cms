use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use argon2::{Argon2, PasswordVerifier, password_hash::PasswordHash};
use axum::{Json as AxumJson, extract::State, http::StatusCode, response::IntoResponse};
use chrono::{DateTime, Local, TimeDelta, Utc};
use jsonwebtoken::{self, DecodingKey, EncodingKey, Header, Validation};
use rand::RngExt;
use real::RealIp;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use tokio::{sync::Mutex, task};
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
    utils::{cmd_args::Args, errors::NurError},
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

fn frontend_name() -> String {
    option_env!("FRONTEND_NAME")
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| "NUR CMS".to_string())
}

/// Create a json web token (JWT)
pub async fn encode_jwt(claims: Claims) -> Result<String, NurError> {
    let encoding_key = EncodingKey::from_secret(CONFIG.read().await.jwt_secret.as_bytes());
    Ok(jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &encoding_key,
    )?)
}

/// Decode a json web token (JWT)
pub async fn decode_jwt(token: &str) -> Result<Claims, NurError> {
    let decoding_key = DecodingKey::from_secret(CONFIG.read().await.jwt_secret.as_bytes());
    jsonwebtoken::decode::<Claims>(token, &decoding_key, &Validation::default())
        .map(|data| data.claims)
        .map_err(|_| NurError::Unauthorized)
}

pub async fn login(
    real_ip: RealIp,
    State((pool, args)): State<(PgPool, Args)>,
    AxumJson(credentials): AxumJson<Credentials>,
) -> Result<impl IntoResponse, NurError> {
    let ip = real_ip.ip();
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
                    && config.mail_user.as_ref().is_some_and(|u| !u.is_empty())
                    && config.mail_password.as_ref().is_some_and(|p| !p.is_empty())
                    && config.mail_smtp.as_ref().is_some_and(|s| !s.is_empty())
                    && !args.disable_two_factor
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

                    let app_name = frontend_name();
                    let text = mail_body(&verification_code, &app_name);

                    let target = MailTarget::new(email, true);
                    let msg = Msg::new(
                        config.mail_user.unwrap(),
                        app_name.clone(),
                        Some(format!("Your {app_name} code is: {verification_code}")),
                        text,
                        target,
                    );

                    message(msg).await?;

                    info!("{ip} Send verification code");

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

                info!("{ip} User {username} login, with role: {}", role.name);

                return Ok((
                    StatusCode::OK,
                    AxumJson(serde_json::json!({
                        "access": access_token,
                        "refresh": refresh_token,
                    })),
                )
                    .into_response());
            }

            error!("{ip} Wrong password for {username}!");

            Ok((
                StatusCode::FORBIDDEN,
                AxumJson(serde_json::json!({
                    "detail": "Incorrect credentials!",
                })),
            )
                .into_response())
        }
        Err(e) => {
            error!("{ip} Login {username} failed! {e}");

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

pub async fn verify(
    real_ip: RealIp,
    State((pool, _)): State<(PgPool, Args)>,
    AxumJson(request): AxumJson<VerifyRequest>,
) -> Result<impl IntoResponse, NurError> {
    let ip = real_ip.ip();
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

            info!(
                "{ip} User {username} verified successfully, with role: {}",
                role
            );

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
            error!("{ip} No verification code found for {username}");
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

pub async fn refresh(
    State((pool, _)): State<(PgPool, Args)>,
    AxumJson(data): AxumJson<TokenRefreshRequest>,
) -> Result<impl IntoResponse, NurError> {
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
                    .ok_or(NurError::Unauthorized)?
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

fn mail_body(verification_code: &str, add_name: &str) -> String {
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
                <p>Enter this code in the <b>{add_name}</b> verification step to finish signing in:</p>
                <p style="padding: 5px; font-size: 20px; font-weight: bold;">{verification_code}</p>
                <p>This code expires in 5 minutes. If you did not request it, you can ignore this email.</p>
                <div>
                    This message was sent automatically by <b>{add_name}</b>.
                </div>
            </div>
        </div>
    </body>
    </html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn with_test_jwt_secret<T>(secret: &str, f: impl std::future::Future<Output = T>) -> T {
        let prev = CONFIG.read().await.clone();
        {
            let mut cfg = CONFIG.write().await;
            cfg.jwt_secret = secret.to_string();
        }

        let result = f.await;

        {
            let mut cfg = CONFIG.write().await;
            *cfg = prev;
        }

        result
    }

    #[tokio::test]
    async fn claims_exp_in_expected_range() {
        let now = Utc::now().timestamp();
        let lifetime_days = 1;
        let claims = Claims::new(42, Role::Admin, lifetime_days);
        let exp = claims.exp;

        let max = now + lifetime_days * 24 * 60 * 60 + 5;
        assert!(exp >= now, "exp should be in the future");
        assert!(exp <= max, "exp should be within expected range");
    }

    #[tokio::test]
    async fn jwt_encode_decode_roundtrip() {
        with_test_jwt_secret("test-secret", async {
            let claims = Claims::new(7, Role::Author, 1);
            let token = encode_jwt(claims.clone()).await.expect("encode ok");
            let decoded = decode_jwt(&token).await.expect("decode ok");

            assert_eq!(decoded.id, claims.id);
            assert_eq!(decoded.role, claims.role);
            assert_eq!(decoded.exp, claims.exp);
        })
        .await;
    }

    #[test]
    fn mail_body_includes_code_and_branding() {
        let code = "1234567";
        let body = mail_body(code, "NUR CMS");

        assert!(body.contains(code));
        assert!(body.contains("NUR CMS"));
        assert!(body.contains("expires in 5 minutes"));
    }

    #[test]
    fn mail_body_different_codes() {
        let codes = vec!["1234567", "9999999", "0000000"];

        for code in codes {
            let body = mail_body(code, "NUR CMS");
            assert!(body.contains(code), "Code {} should be in mail body", code);
        }
    }

    #[tokio::test]
    async fn claims_new_with_different_roles() {
        let user_id = 123;
        let lifetime = 7;

        let admin_claims = Claims::new(user_id, Role::Admin, lifetime);
        let author_claims = Claims::new(user_id, Role::Author, lifetime);

        assert_eq!(admin_claims.id, user_id);
        assert_eq!(admin_claims.role, Role::Admin);
        assert_eq!(author_claims.id, user_id);
        assert_eq!(author_claims.role, Role::Author);
    }

    #[tokio::test]
    async fn jwt_different_users_produce_different_tokens() {
        with_test_jwt_secret("test-secret", async {
            let claims1 = Claims::new(1, Role::Admin, 1);
            let claims2 = Claims::new(2, Role::Admin, 1);

            let token1 = encode_jwt(claims1).await.expect("encode ok");
            let token2 = encode_jwt(claims2).await.expect("encode ok");

            assert_ne!(
                token1, token2,
                "Different user IDs should produce different tokens"
            );

            let decoded1 = decode_jwt(&token1).await.expect("decode ok");
            let decoded2 = decode_jwt(&token2).await.expect("decode ok");

            assert_eq!(decoded1.id, 1);
            assert_eq!(decoded2.id, 2);
        })
        .await;
    }

    #[tokio::test]
    async fn jwt_invalid_token_fails_decode() {
        with_test_jwt_secret("test-secret", async {
            let invalid_token = "invalid.token.here";
            let result = decode_jwt(invalid_token).await;

            assert!(result.is_err(), "Invalid token should fail to decode");
        })
        .await;
    }

    #[tokio::test]
    async fn jwt_tampered_token_fails_decode() {
        with_test_jwt_secret("test-secret", async {
            let claims = Claims::new(1, Role::Admin, 1);
            let token = encode_jwt(claims).await.expect("encode ok");

            // Try to decode with different secret
            let decoding_key = DecodingKey::from_secret("wrong-secret".as_bytes());
            let result =
                jsonwebtoken::decode::<Claims>(&token, &decoding_key, &Validation::default());

            assert!(result.is_err(), "Token with wrong secret should fail");
        })
        .await;
    }

    #[tokio::test]
    async fn verification_code_struct_creation() {
        let now = Utc::now();
        let code = VerificationCode {
            code: "1234567".to_string(),
            user_id: 42,
            role: Role::Author,
            created_at: now,
        };

        assert_eq!(code.code, "1234567");
        assert_eq!(code.user_id, 42);
        assert_eq!(code.role, Role::Author);
        assert_eq!(code.created_at, now);
    }

    #[tokio::test]
    async fn verification_code_expiry_check_fresh() {
        let now = Utc::now();
        let code = VerificationCode {
            code: "1234567".to_string(),
            user_id: 42,
            role: Role::Author,
            created_at: now,
        };

        let elapsed = Utc::now().signed_duration_since(code.created_at);
        assert!(
            elapsed.num_minutes() <= 5,
            "Fresh code should not be expired"
        );
    }

    #[tokio::test]
    async fn verification_code_expires_after_5_minutes() {
        let far_past = Utc::now() - chrono::Duration::minutes(6);
        let code = VerificationCode {
            code: "1234567".to_string(),
            user_id: 42,
            role: Role::Author,
            created_at: far_past,
        };

        let elapsed = Utc::now().signed_duration_since(code.created_at);
        assert!(
            elapsed.num_minutes() > 5,
            "Code older than 5 minutes should be expired"
        );
    }

    #[tokio::test]
    async fn credentials_struct_creation() {
        let creds = Credentials {
            username: "testuser".to_string(),
            password: "testpass".to_string(),
        };

        assert_eq!(creds.username, "testuser");
        assert_eq!(creds.password, "testpass");
    }

    #[tokio::test]
    async fn verify_request_struct_creation() {
        let req = VerifyRequest {
            username: "testuser".to_string(),
            code: "1234567".to_string(),
        };

        assert_eq!(req.username, "testuser");
        assert_eq!(req.code, "1234567");
    }

    #[tokio::test]
    async fn token_refresh_request_struct_creation() {
        let req = TokenRefreshRequest {
            refresh: "refresh.token.here".to_string(),
        };

        assert_eq!(req.refresh, "refresh.token.here");
    }

    #[tokio::test]
    async fn claims_exp_increases_with_lifetime() {
        let user_id = 1;
        let role = Role::Admin;
        let now = Utc::now().timestamp();

        let claims_1day = Claims::new(user_id, role.clone(), 1);
        let claims_7day = Claims::new(user_id, role.clone(), 7);
        let claims_30day = Claims::new(user_id, role.clone(), 30);

        // Verify that longer lifetimes produce later expiration timestamps
        assert!(claims_1day.exp < claims_7day.exp);
        assert!(claims_7day.exp < claims_30day.exp);

        // Verify first claim is roughly 1 day from now
        let expected_1day = now + (24 * 60 * 60);
        assert!(
            (claims_1day.exp - expected_1day).abs() < 5,
            "1-day claim should expire ~24h from now"
        );
    }

    #[tokio::test]
    async fn jwt_encode_decode_preserves_all_fields() {
        with_test_jwt_secret("test-secret", async {
            let original_id = 999;
            let original_role = Role::Author;
            let lifetime = 10;

            let claims = Claims::new(original_id, original_role.clone(), lifetime);
            let original_exp = claims.exp;

            let token = encode_jwt(claims).await.expect("encode ok");
            let decoded = decode_jwt(&token).await.expect("decode ok");

            assert_eq!(decoded.id, original_id, "ID should be preserved");
            assert_eq!(decoded.role, original_role, "Role should be preserved");
            assert_eq!(
                decoded.exp, original_exp,
                "Expiration should be preserved exactly"
            );
        })
        .await;
    }
}
