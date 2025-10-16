use std::{
    fmt::{Debug, Display},
    str::FromStr,
};

use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
use chrono::{DateTime, Utc};
use colored::Colorize;
use serde::Serialize;
use serde_json::Value;
use sqlx::{Execute, Pool, Postgres, QueryBuilder, postgres::PgPool};
use strum::IntoEnumIterator;
use tokio::task;
use tracing::{debug, error, warn};

use crate::{
    db::{
        fields::{AuthUserFields, BlogPostFields, ColumnCounter, StrCompare, Table},
        format_sql,
        models::AuthUser,
        queries::{QueryObj, RespondObj, where_chain},
        serialize::{AuthUserSerializer, BlogPostSerializer},
    },
    utils::errors::ServiceError,
};

#[cfg(debug_assertions)]
pub async fn dev_user(pool: &PgPool) -> Result<(), ServiceError> {
    let user = AuthUser::new(
        "admin@example.org".to_string(),
        "admin".to_string(),
        "Ad".to_string(),
        "Min".to_string(),
        "admin".to_string(),
        1,
    );

    insert_record(pool, &Table::AuthUsers, &user).await?;

    Ok(())
}

pub async fn db_migrate(pool: &Pool<Postgres>) -> Result<(), ServiceError> {
    sqlx::migrate!("../migrations").run(pool).await?;

    #[cfg(debug_assertions)]
    dev_user(pool).await?;

    Ok(())
}

pub async fn select_auth_user(
    pool: &Pool<Postgres>,
    query_obj: QueryObj<AuthUserFields>,
) -> Result<RespondObj<AuthUserSerializer>, ServiceError> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut separated = query_builder.separated(", ");

    for f in &query_obj.fields {
        if *f != AuthUserFields::Role {
            separated.push(format!("u.{f}"));
        }
    }

    separated.push("(r.id, r.name) AS \"auth_role\"");
    separated.push("count(*) OVER() AS total_count");

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

    let query = query_builder.build_query_as::<AuthUserSerializer>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()).bright_black());

    let data: Vec<AuthUserSerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(&query_obj, data))
}

pub async fn delete_record(pool: &PgPool, table: &Table, id: i32) -> Result<(), ServiceError> {
    let mut qb = QueryBuilder::<Postgres>::new(format!("DELETE FROM {table} WHERE id = "));
    qb.push_bind(id);

    let query = qb.build();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()).bright_black());

    let rows_affected = query.execute(pool).await?.rows_affected();

    if rows_affected == 0 {
        let msg = format!(
            "No record with id={} found in {}",
            id.to_string().yellow(),
            table.to_string().purple()
        );
        warn!("{msg}");
        return Err(ServiceError::UnprocessableEntity(msg));
    }

    debug!(
        "Deleted record with id={} from {}",
        id.to_string().yellow(),
        table.to_string().purple()
    );

    Ok(())
}

pub async fn insert_record<T>(pool: &PgPool, table: &Table, data: &T) -> Result<i32, ServiceError>
where
    T: Serialize,
{
    let value = serde_json::to_value(data)?;

    let obj = match value.as_object() {
        Some(map) => map.clone(),
        None => return Err(ServiceError::NoContent),
    };

    let type_ignore = ["id", "created_at", "updated_at", "last_login"];

    let mut keys = Vec::new();
    let mut qb = QueryBuilder::<Postgres>::new(format!("INSERT INTO {table} ("));

    for key in obj.keys() {
        if type_ignore.contains(&key.as_str()) {
            continue;
        }
        keys.push(key.clone());
    }

    if keys.is_empty() {
        return Err(ServiceError::NoContent);
    }

    qb.push(keys.join(", "));
    qb.push(") VALUES (");

    let mut separated = qb.separated(", ");
    for key in &keys {
        let val = &obj[key];

        match val {
            Value::Array(a) => {
                separated.push_bind(a);
            }
            Value::Bool(b) => {
                separated.push_bind(b);
            }
            Value::Null => {
                separated.push_bind("DEFAULT");
            }
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    separated.push_bind(i as i32);
                } else if let Some(f) = n.as_f64() {
                    separated.push_bind(f);
                }
            }
            Value::String(s) => {
                if key == "password" {
                    let pw = s.clone();
                    let password_hash =
                        task::spawn_blocking(move || -> Result<String, ServiceError> {
                            let salt = SaltString::generate(&mut OsRng);
                            let hash = Argon2::default()
                                .hash_password(pw.as_bytes(), &salt)
                                .map_err(|_| ServiceError::InternalServerError)?;

                            Ok(hash.to_string())
                        })
                        .await??;

                    separated.push_bind(password_hash);
                } else {
                    separated.push_bind(s);
                }
            }
            other => {
                error!("Unknown Type {key}={other:?} in Insert!");
                separated.push_bind("DEFAULT");
            }
        }
    }

    qb.push(") RETURNING id");

    let query = qb.build_query_scalar();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()).bright_black());

    let id = query.fetch_one(pool).await?;

    Ok(id)
}

pub async fn select_record<T, M>(
    pool: &Pool<Postgres>,
    table: &Table,
    query_obj: QueryObj<T>,
) -> Result<RespondObj<M>, ServiceError>
where
    T: Display + StrCompare + IntoEnumIterator + FromStr + Debug,
    M: for<'a> sqlx::FromRow<'a, sqlx::postgres::PgRow> + Send + Unpin + ColumnCounter,
{
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut separated = query_builder.separated(", ");

    for f in &query_obj.fields {
        separated.push(f.to_owned());
    }

    separated.push("count(*) OVER() AS total_count");
    query_builder.push(format!(" FROM {table}"));

    if let Some(id) = &query_obj.search_id {
        where_chain(&mut query_builder, None, "id = ");
        query_builder.push_bind(id);
    }

    if query_obj
        .fields
        .iter()
        .any(|f| query_obj.ordering.contains(&f.to_string()))
    {
        let ordering = query_obj
            .ordering
            .split(", ")
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        query_builder.push(format!(" ORDER BY {ordering}"));
    }

    query_builder.push(format!(
        " LIMIT {} OFFSET {}",
        query_obj.limit, query_obj.offset
    ));

    let query = query_builder.build_query_as::<M>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()).bright_black());

    let data: Vec<M> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(&query_obj, data))
}

pub async fn update_record<T>(
    pool: &PgPool,
    table: &Table,
    id: i32,
    data: &T,
) -> Result<(), ServiceError>
where
    T: Serialize,
{
    let value = serde_json::to_value(data)?;

    let obj = match value.as_object() {
        Some(map) => map.clone(),
        None => return Ok(()),
    };

    let type_ignore = ["created_at"];
    let type_time = ["updated_at", "last_login"];

    let mut qb = QueryBuilder::<Postgres>::new(format!("UPDATE {table} SET "));
    let mut separated = qb.separated(", ");
    let mut any_field = false;

    for (key, val) in obj {
        if val.is_null() || type_ignore.contains(&key.as_str()) {
            continue;
        }
        any_field = true;

        separated.push(format!("{key} = "));

        match val {
            Value::Array(a) => {
                separated.push_bind_unseparated(a);
            }
            Value::String(s) => {
                if type_time.contains(&key.as_str()) {
                    let dt: DateTime<Utc> = match DateTime::parse_from_rfc3339(&s) {
                        Ok(t) => t.into(),
                        Err(_) => Utc::now(),
                    };

                    separated.push_bind_unseparated(dt);
                } else if key.as_str() == "password" {
                    let pw = s.clone();
                    let password_hash =
                        task::spawn_blocking(move || -> Result<String, ServiceError> {
                            let salt = SaltString::generate(&mut OsRng);
                            let hash = Argon2::default()
                                .hash_password(pw.as_bytes(), &salt)
                                .map_err(|_| ServiceError::InternalServerError)?;

                            Ok(hash.to_string())
                        })
                        .await??;

                    separated.push_bind_unseparated(password_hash);
                } else {
                    separated.push_bind_unseparated(s);
                }
            }
            Value::Bool(b) => {
                separated.push_bind_unseparated(b);
            }
            Value::Number(n) => {
                separated.push_bind_unseparated(n.as_i64().unwrap_or_default() as i32);
            }
            _ => {
                error!("Unknown Type {key}={val:?} in Update!");
                continue;
            }
        }
    }

    if !any_field {
        return Ok(());
    }

    qb.push(" WHERE id = ");
    qb.push_bind(id);

    let query = qb.build();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()).bright_black());

    query.execute(pool).await?;

    Ok(())
}

/* ------------------------------------
BLOG POSTS
--------------------------------------- */

pub async fn select_content(
    pool: &Pool<Postgres>,
    query_obj: QueryObj<BlogPostFields>,
) -> Result<RespondObj<BlogPostSerializer>, ServiceError> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut separated = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            BlogPostFields::Author => {
                separated.push(format!("(u.id, u.first_name, u.last_name) AS {f}"))
            }
            BlogPostFields::Title => separated.push(format!("fv_title.value->>0 AS {f}")),
            BlogPostFields::Body => separated.push(format!("fv_body.value->>0 AS {f}")),
            BlogPostFields::Locale => separated.push(format!("l.code as {f}")),
            _ => separated.push(format!("ci.{f}")),
        };
    }

    separated.push("count(*) OVER() AS total_count");
    separated.push_unseparated(" ");
    query_builder.push("FROM content_items ci ");
    query_builder.push("JOIN content_types ct ON ct.id = ci.content_type_id ");

    if query_obj.fields.contains(&BlogPostFields::Author) {
        query_builder.push("LEFT JOIN auth_users u ON u.id = ci.created_by ");
    }

    if query_obj.fields.contains(&BlogPostFields::Title) {
        query_builder.push(
            r#"LEFT JOIN content_values fv_title
            ON fv_title.content_item_id = ci.id
                AND fv_title.locale_id = 1
                AND fv_title.field_id = (
                    SELECT id FROM content_fields
                    WHERE content_type_id = ct.id AND name = 'title'
                ) "#,
        );
    }

    if query_obj.fields.contains(&BlogPostFields::Body) {
        query_builder.push(
            r#"LEFT JOIN content_values fv_body
            ON fv_body.content_item_id = ci.id
                AND fv_body.locale_id = 1
                AND fv_body.field_id = (
                    SELECT id FROM content_fields
                    WHERE content_type_id = ct.id AND name = 'body'
                ) "#,
        );
    }

    if query_obj.fields.contains(&BlogPostFields::Locale) {
        query_builder.push("JOIN locales l ON l.id = fv_title.locale_id ");
    }

    if let Some(id) = &query_obj.search_id {
        where_chain(&mut query_builder, None, "ci.id = ");
        query_builder.push_bind(id);
    }

    if let Some(after) = &query_obj.created_after {
        where_chain(&mut query_builder, None, "ci.created_at >= ");
        query_builder.push_bind(after);
    }

    if let Some(before) = &query_obj.created_before {
        where_chain(&mut query_builder, None, "ci.created_at < ");
        query_builder.push_bind(before);
    }

    if let Some(sl) = &query_obj.type_slag {
        where_chain(&mut query_builder, None, "ct.slug = ");
        query_builder.push_bind(sl.to_string());
    }

    if let Some(url) = &query_obj.url_slag {
        where_chain(&mut query_builder, None, "ci.slug = ");
        query_builder.push_bind(url);
    }

    if let Some(search) = query_obj.search.clone() {
        where_chain(&mut query_builder, None, "title LIKE ");
        query_builder.push("CONCAT('%', ");
        query_builder.push_bind(search.clone());
        query_builder.push(", '%')");

        // TODO: add full text search
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

    let query = query_builder.build_query_as::<BlogPostSerializer>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()).bright_black());

    let data: Vec<BlogPostSerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(&query_obj, data))
}
