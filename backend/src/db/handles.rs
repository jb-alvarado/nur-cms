use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
use chrono::Utc;
use sqlx::{Execute, Pool, Postgres, QueryBuilder};
use tokio::task;
use tracing::debug;

use crate::{
    db::{
        format_sql,
        models::{AuthRole, AuthUser},
        queries::{QueryObj, RespondObj, where_chain},
    },
    utils::errors::ServiceError,
};

pub async fn db_migrate(pool: &Pool<Postgres>) -> Result<(), ServiceError> {
    sqlx::migrate!("../migrations").run(pool).await?;

    Ok(())
}

pub async fn select_auth_role(
    pool: &Pool<Postgres>,
    id: Option<&i32>,
) -> Result<Vec<AuthRole>, ServiceError> {
    let result = match id {
        Some(id) => {
            sqlx::query_as("SELECT id, name FROM auth_roles WHERE id = $1")
                .bind(id)
                .fetch_all(pool)
                .await?
        }
        None => {
            sqlx::query_as("SELECT id, name FROM auth_roles")
                .fetch_all(pool)
                .await?
        }
    };

    Ok(result)
}

pub async fn select_auth_user(
    pool: &Pool<Postgres>,
    id: Option<i32>,
    user: Option<&str>,
) -> Result<AuthUser, ServiceError> {
    const QUERY: &str = r#"
        SELECT
            id, email, username, password, created_at, last_login, role_id
        FROM
            auth_users
        WHERE
            id = $1
        OR
            username = $2
        OR
            email = $2;
        "#;

    let result = sqlx::query_as(QUERY)
        .bind(id)
        .bind(user)
        .fetch_one(pool)
        .await?;

    Ok(result)
}

pub async fn insert_auth_user(pool: &Pool<Postgres>, user: AuthUser) -> Result<(), ServiceError> {
    const QUERY: &str =
        "INSERT INTO auth_users (email, username, password, role_id) VALUES ($1, $2, $3, $4)";

    // Hash password in blocking thread, return ServiceError on failure instead of panicking
    let password_hash = task::spawn_blocking(move || -> Result<String, ServiceError> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(user.password.as_bytes(), &salt)
            .map_err(|_| ServiceError::InternalServerError)?;

        Ok(hash.to_string())
    })
    .await??;

    sqlx::query(QUERY)
        .bind(user.email)
        .bind(user.username)
        .bind(password_hash)
        .bind(user.role_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn update_auth_user_last_login(
    conn: &Pool<Postgres>,
    user_id: i32,
) -> Result<(), ServiceError> {
    let query = "UPDATE auth_users SET last_login = $1 WHERE id = $2;";

    sqlx::query(query)
        .bind(Utc::now())
        .bind(user_id)
        .execute(conn)
        .await?;

    Ok(())
}
