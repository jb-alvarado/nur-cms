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
        fields::AuthUserFields,
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
    query_obj: QueryObj<AuthUserFields>,
) -> Result<RespondObj<AuthUser>, ServiceError> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut separated = query_builder.separated(", ");

    for f in &query_obj.fields {
        if *f != AuthUserFields::Role {
            separated.push(format!("u.{f}"));
        }
    }

    separated.push("(r.id, r.name) AS \"auth_role\"");

    separated.push("count(*) OVER() AS total_count".to_string());

    separated.push_unseparated(" ");
    query_builder.push("FROM auth_users u ");

    if query_obj.fields.contains(&AuthUserFields::Role) {
        query_builder.push("LEFT JOIN auth_roles r ON r.id = u.role_id");
    }

    if let Some(id) = &query_obj.search_id {
        where_chain(&mut query_builder, None, "u.id = ");
        query_builder.push_bind(id);
    }

    if let Some(after) = &query_obj.created_after {
        where_chain(&mut query_builder, None, "u.created_at >= ");
        query_builder.push_bind(after);
    }

    if let Some(before) = &query_obj.created_before {
        where_chain(&mut query_builder, None, "u.created_at < ");
        query_builder.push_bind(before);
    }

    if let Some(search) = query_obj.search.clone() {
        where_chain(&mut query_builder, None, "u.username LIKE ");
        query_builder.push("CONCAT('%', ");
        query_builder.push_bind(search.clone());
        query_builder.push(", '%')");

        where_chain(&mut query_builder, Some(" OR"), "u.email LIKE ");
        query_builder.push("CONCAT('%', ");
        query_builder.push_bind(search.clone());
        query_builder.push(", '%')");
    }

    if query_obj
        .fields
        .iter()
        .any(|f| query_obj.ordering.contains(&f.to_string()))
    {
        let ordering = query_obj
            .ordering
            .split(", ")
            .map(|item| format!("u.{}", item))
            .collect::<Vec<_>>()
            .join(", ");
        query_builder.push(format!(" ORDER BY {}", ordering));
    }

    query_builder.push(format!(
        " LIMIT {} OFFSET {}",
        query_obj.limit, query_obj.offset
    ));

    let query = query_builder.build_query_as::<AuthUser>();

    #[cfg(debug_assertions)]
    debug!("\n{}", format_sql(query.sql()));

    let data: Vec<AuthUser> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(&query_obj, data))
}

pub async fn insert_auth_user(pool: &Pool<Postgres>, user: AuthUser) -> Result<(), ServiceError> {
    const QUERY: &str =
        "INSERT INTO auth_users (email, username, password, role_id) VALUES ($1, $2, $3, $4)";

    // Hash password in blocking thread, return ServiceError on failure instead of panicking
    let password_hash = task::spawn_blocking(move || -> Result<String, ServiceError> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(user.password.unwrap().as_bytes(), &salt)
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
