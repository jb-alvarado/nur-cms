use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use argon2::{
    Argon2, PasswordHasher, PasswordVerifier,
    password_hash::{PasswordHash, SaltString, rand_core::OsRng},
};
use axum::{Json as AxumJson, extract::State, http::StatusCode, response::IntoResponse};
use chrono::{DateTime, TimeDelta, Utc};
use jsonwebtoken::{self, DecodingKey, EncodingKey, Header, Validation};
use rand::RngExt;
use real::RealIp;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use tokio::{sync::Mutex, task};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    ACCESS_LIFETIME_MINUTES, CONFIG, REFRESH_LIFETIME,
    db::{
        fields::AuthUserFields,
        handles,
        models::{MailTarget, Role},
        queries::QueryObj,
    },
    mail::client::{Msg, message},
    utils::{cmd_args::Args, errors::NurError},
};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Claims {
    pub id: i32,
    pub role: Role,
    pub token_type: TokenType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    iat: i64,
    exp: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Access,
    Refresh,
}

impl Claims {
    pub fn access(id: i32, role: Role) -> Self {
        let now = Utc::now();
        Self {
            id,
            role,
            token_type: TokenType::Access,
            jti: None,
            iat: now.timestamp(),
            exp: (now
                + TimeDelta::try_minutes(*ACCESS_LIFETIME_MINUTES)
                    .expect("access-token lifetime is bounded"))
            .timestamp(),
        }
    }

    pub fn refresh(id: i32, role: Role, jti: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            role,
            token_type: TokenType::Refresh,
            jti: Some(jti),
            iat: now.timestamp(),
            exp: (now
                + TimeDelta::try_days(*REFRESH_LIFETIME)
                    .expect("refresh-token lifetime is bounded"))
            .timestamp(),
        }
    }
}

#[derive(Debug, Serialize)]
struct TokenPair {
    access: String,
    refresh: String,
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
    pub failed_attempts: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VerifyRequest {
    pub username: String,
    pub code: String,
}

// Global storage for verification codes
pub static VERIFICATION_CODES: LazyLock<Arc<Mutex<HashMap<String, VerificationCode>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

static DUMMY_PASSWORD_HASH: LazyLock<String> = LazyLock::new(|| {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(b"nur-cms-dummy-password", &salt)
        .expect("dummy password hash must be valid")
        .to_string()
});

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
async fn decode_jwt_with_type(token: &str, expected: TokenType) -> Result<Claims, NurError> {
    let decoding_key = DecodingKey::from_secret(CONFIG.read().await.jwt_secret.as_bytes());
    let claims = jsonwebtoken::decode::<Claims>(token, &decoding_key, &Validation::default())
        .map(|data| data.claims)
        .map_err(|_| NurError::Unauthorized)?;

    if claims.token_type != expected || claims.iat <= 0 || claims.exp <= claims.iat {
        return Err(NurError::Unauthorized);
    }

    let maximum_lifetime = match expected {
        TokenType::Access => ACCESS_LIFETIME_MINUTES.saturating_mul(60),
        TokenType::Refresh => REFRESH_LIFETIME.saturating_mul(24 * 60 * 60),
    };
    if claims.exp - claims.iat > maximum_lifetime
        || (expected == TokenType::Refresh && claims.jti.is_none())
    {
        return Err(NurError::Unauthorized);
    }

    Ok(claims)
}

/// Decode an access token used to authorize API requests.
pub async fn decode_jwt(token: &str) -> Result<Claims, NurError> {
    decode_jwt_with_type(token, TokenType::Access).await
}

/// Decode a refresh token used only by the refresh and logout endpoints.
pub async fn decode_refresh_jwt(token: &str) -> Result<Claims, NurError> {
    decode_jwt_with_type(token, TokenType::Refresh).await
}

async fn issue_token_pair(pool: &PgPool, user_id: i32, role: Role) -> Result<TokenPair, NurError> {
    let jti = Uuid::new_v4().to_string();
    let access = encode_jwt(Claims::access(user_id, role.clone())).await?;
    let refresh_claims = Claims::refresh(user_id, role, jti.clone());
    let expires_at = refresh_claims.exp;
    let refresh = encode_jwt(refresh_claims).await?;
    handles::insert_refresh_token(
        pool,
        &jti,
        &jti,
        user_id,
        expires_at,
        Utc::now().timestamp(),
    )
    .await?;

    Ok(TokenPair { access, refresh })
}

async fn rotate_token_pair(
    pool: &PgPool,
    old_claims: &Claims,
    user_id: i32,
    role: Role,
) -> Result<Option<TokenPair>, NurError> {
    let Some(old_jti) = old_claims.jti.as_deref() else {
        return Ok(None);
    };
    let new_jti = Uuid::new_v4().to_string();
    let access = encode_jwt(Claims::access(user_id, role.clone())).await?;
    let refresh_claims = Claims::refresh(user_id, role, new_jti.clone());
    let expires_at = refresh_claims.exp;
    let refresh = encode_jwt(refresh_claims).await?;
    let rotation = handles::rotate_refresh_token(
        pool,
        old_jti,
        &new_jti,
        user_id,
        expires_at,
        Utc::now().timestamp(),
    )
    .await?;

    Ok((rotation == handles::RefreshRotation::Rotated).then_some(TokenPair { access, refresh }))
}

pub async fn login(
    real_ip: RealIp,
    State((pool, args)): State<(PgPool, Args)>,
    AxumJson(credentials): AxumJson<Credentials>,
) -> Result<impl IntoResponse, NurError> {
    let ip = real_ip.ip();
    let username = credentials.username.clone();
    let password = credentials.password.clone();
    if username.is_empty() || username.len() > 150 || password.is_empty() || password.len() > 1_024
    {
        return Err(NurError::BadRequest("Invalid credentials.".into()));
    }
    match handles::select_auth_user_for_login(&pool, &username).await {
        Ok(user) => {
            let Some(mut user) = user else {
                let cred_password = password.clone();
                let _ = task::spawn_blocking(move || {
                    let hash = PasswordHash::new(&DUMMY_PASSWORD_HASH)?;
                    Argon2::default().verify_password(cred_password.as_bytes(), &hash)
                })
                .await;
                return Ok((
                    StatusCode::FORBIDDEN,
                    AxumJson(serde_json::json!({
                        "detail": "Incorrect credentials!",
                    })),
                )
                    .into_response());
            };

            let role = user.role.clone().ok_or(NurError::Unauthorized)?;
            let user_id = user.id.ok_or(NurError::Unauthorized)?;

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

                if !args.disable_two_factor {
                    let email = user
                        .email
                        .clone()
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            NurError::ServiceUnavailable(
                                "Two-factor authentication is not configured.".into(),
                            )
                        })?;
                    let mail_user = config
                        .mail_user
                        .clone()
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            NurError::ServiceUnavailable(
                                "Two-factor authentication is not configured.".into(),
                            )
                        })?;
                    if config.mail_password.as_ref().is_none_or(String::is_empty)
                        || config.mail_smtp.as_ref().is_none_or(String::is_empty)
                    {
                        return Err(NurError::ServiceUnavailable(
                            "Two-factor authentication is not configured.".into(),
                        ));
                    }

                    // Generate 7-digit random code
                    let verification_code: String = (0..7)
                        .map(|_| rand::rng().random_range(0..10).to_string())
                        .collect();

                    // Store code with timestamp
                    let verification_entry = VerificationCode {
                        code: verification_code.clone(),
                        user_id,
                        role: role.name.clone(),
                        created_at: Utc::now(),
                        failed_attempts: 0,
                    };

                    VERIFICATION_CODES
                        .lock()
                        .await
                        .insert(username.clone(), verification_entry);

                    // Start cleanup task for this code
                    let username_cleanup = username.clone();
                    let verification_code_cleanup = verification_code.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(300)).await; // 5 minutes
                        let mut codes = VERIFICATION_CODES.lock().await;
                        if codes
                            .get(&username_cleanup)
                            .is_some_and(|entry| entry.code == verification_code_cleanup)
                        {
                            codes.remove(&username_cleanup);
                            info!("Verification code for {username_cleanup} expired and removed");
                        }
                    });

                    let app_name = frontend_name();
                    let text = mail_body(&verification_code, &app_name);

                    let target = MailTarget::new(email, true);
                    let msg = Msg::new(
                        mail_user,
                        app_name.clone(),
                        Some(format!("Your {app_name} code is: {verification_code}")),
                        text,
                        target,
                    );

                    if let Err(error) = message(msg).await {
                        let mut codes = VERIFICATION_CODES.lock().await;
                        if codes
                            .get(&username)
                            .is_some_and(|entry| entry.code == verification_code)
                        {
                            codes.remove(&username);
                        }
                        return Err(error);
                    }

                    info!("{ip} Send verification code");

                    return Ok((
                        StatusCode::OK,
                        AxumJson(serde_json::json!({
                            "detail": "Verification code sended to email!",
                        })),
                    )
                        .into_response());
                }

                warn!("Two-factor authentication is explicitly disabled");

                let tokens = issue_token_pair(&pool, user_id, role.name.clone()).await?;
                handles::update_last_login(&pool, user_id).await?;

                info!("{ip} User {username} login, with role: {}", role.name);

                return Ok((
                    StatusCode::OK,
                    AxumJson(serde_json::json!({
                        "access": tokens.access,
                        "refresh": tokens.refresh,
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
    if username.is_empty()
        || username.len() > 150
        || provided_code.len() != 7
        || !provided_code.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(NurError::BadRequest("Invalid verification request.".into()));
    }

    // Check if code exists
    let verification_data = {
        let mut codes = VERIFICATION_CODES.lock().await;

        if let Some(mut verification) = codes.remove(&username) {
            // Check if code is still valid (max 5 minutes)
            let elapsed = Utc::now().signed_duration_since(verification.created_at);
            if elapsed >= TimeDelta::minutes(5) {
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
                verification.failed_attempts = verification.failed_attempts.saturating_add(1);
                if verification.failed_attempts < 5 {
                    codes.insert(username.clone(), verification);
                }
                return Ok((
                    StatusCode::FORBIDDEN,
                    AxumJson(serde_json::json!({
                        "detail": "Invalid verification code!",
                    })),
                )
                    .into_response());
            }

            // Code is valid, remove it and return data
            Some(verification)
        } else {
            None
        }
    };

    match verification_data {
        Some(verification) => {
            let user_id = verification.user_id;
            let role = verification.role;

            let tokens = issue_token_pair(&pool, user_id, role.clone()).await?;

            // Update last_login
            handles::update_last_login(&pool, user_id).await?;

            info!(
                "{ip} User {username} verified successfully, with role: {}",
                role
            );

            Ok((
                StatusCode::OK,
                AxumJson(serde_json::json!({
                    "access": tokens.access,
                    "refresh": tokens.refresh,
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
    match decode_refresh_jwt(&data.refresh).await {
        Ok(claims) => {
            let user_id = claims.id;

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
                let username = resp.results[0].username.clone().unwrap_or_default();
                let role_name = resp.results[0]
                    .role
                    .clone()
                    .ok_or(NurError::Unauthorized)?
                    .name;

                let Some(tokens) =
                    rotate_token_pair(&pool, &claims, user_id, role_name.clone()).await?
                else {
                    return Ok((
                        StatusCode::FORBIDDEN,
                        AxumJson(serde_json::json!({
                            "detail": "Invalid refresh token",
                        })),
                    ));
                };

                info!("user {username} refresh, with role: {role_name}");

                return Ok((
                    StatusCode::OK,
                    AxumJson(serde_json::json!({
                        "access": tokens.access,
                        "refresh": tokens.refresh,
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

/// Revoke the refresh-token family for the current session.
pub async fn logout(
    State((pool, _)): State<(PgPool, Args)>,
    AxumJson(data): AxumJson<TokenRefreshRequest>,
) -> Result<StatusCode, NurError> {
    if let Ok(claims) = decode_refresh_jwt(&data.refresh).await
        && let Some(jti) = claims.jti
    {
        handles::revoke_refresh_family(&pool, &jti, claims.id, Utc::now().timestamp()).await?;
    }

    Ok(StatusCode::NO_CONTENT)
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

    static TEST_JWT_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    async fn with_test_jwt_secret<T>(secret: &str, f: impl std::future::Future<Output = T>) -> T {
        let _guard = TEST_JWT_LOCK.lock().await;
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
    async fn access_claims_have_the_configured_lifetime() {
        let now = Utc::now().timestamp();
        let claims = Claims::access(42, Role::Admin);
        let expected = *ACCESS_LIFETIME_MINUTES * 60;

        assert_eq!(claims.token_type, TokenType::Access);
        assert!(claims.jti.is_none());
        assert!((claims.exp - claims.iat - expected).abs() <= 1);
        assert!(claims.iat >= now);
    }

    #[tokio::test]
    async fn jwt_encode_decode_roundtrip() {
        with_test_jwt_secret("test-secret", async {
            let claims = Claims::access(7, Role::Author);
            let token = encode_jwt(claims.clone()).await.expect("encode ok");
            let decoded = decode_jwt(&token).await.expect("decode ok");

            assert_eq!(decoded.id, claims.id);
            assert_eq!(decoded.role, claims.role);
            assert_eq!(decoded.exp, claims.exp);
        })
        .await;
    }

    #[tokio::test]
    async fn access_and_refresh_tokens_are_not_interchangeable() {
        with_test_jwt_secret("test-secret", async {
            let access = encode_jwt(Claims::access(7, Role::Author))
                .await
                .expect("encode access");
            let refresh = encode_jwt(Claims::refresh(7, Role::Author, Uuid::new_v4().to_string()))
                .await
                .expect("encode refresh");

            assert!(decode_jwt(&access).await.is_ok());
            assert!(decode_refresh_jwt(&refresh).await.is_ok());
            assert!(decode_jwt(&refresh).await.is_err());
            assert!(decode_refresh_jwt(&access).await.is_err());
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
    async fn access_claims_preserve_roles() {
        let user_id = 123;

        let admin_claims = Claims::access(user_id, Role::Admin);
        let author_claims = Claims::access(user_id, Role::Author);

        assert_eq!(admin_claims.id, user_id);
        assert_eq!(admin_claims.role, Role::Admin);
        assert_eq!(author_claims.id, user_id);
        assert_eq!(author_claims.role, Role::Author);
    }

    #[tokio::test]
    async fn jwt_different_users_produce_different_tokens() {
        with_test_jwt_secret("test-secret", async {
            let claims1 = Claims::access(1, Role::Admin);
            let claims2 = Claims::access(2, Role::Admin);

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
            let claims = Claims::access(1, Role::Admin);
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
            failed_attempts: 0,
        };

        assert_eq!(code.code, "1234567");
        assert_eq!(code.user_id, 42);
        assert_eq!(code.role, Role::Author);
        assert_eq!(code.created_at, now);
        assert_eq!(code.failed_attempts, 0);
    }

    #[tokio::test]
    async fn verification_code_expiry_check_fresh() {
        let now = Utc::now();
        let code = VerificationCode {
            code: "1234567".to_string(),
            user_id: 42,
            role: Role::Author,
            created_at: now,
            failed_attempts: 0,
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
            failed_attempts: 0,
        };

        let elapsed = Utc::now().signed_duration_since(code.created_at);
        assert!(
            elapsed >= chrono::Duration::minutes(5),
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
    async fn refresh_claims_have_a_jti_and_longer_lifetime() {
        let access = Claims::access(1, Role::Admin);
        let refresh = Claims::refresh(1, Role::Admin, Uuid::new_v4().to_string());

        assert_eq!(refresh.token_type, TokenType::Refresh);
        assert!(refresh.jti.is_some());
        assert_eq!(refresh.exp - refresh.iat, *REFRESH_LIFETIME * 24 * 60 * 60);
        assert!(refresh.exp > access.exp);
    }

    #[tokio::test]
    async fn jwt_encode_decode_preserves_all_fields() {
        with_test_jwt_secret("test-secret", async {
            let original_id = 999;
            let original_role = Role::Author;
            let claims = Claims::access(original_id, original_role.clone());
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
