use std::{
    fmt::{Debug, Display},
    path::Path,
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
use sqlx::{Execute, Postgres, QueryBuilder, postgres::PgPool};
use strum::IntoEnumIterator;
use tokio::{fs, task};
use tracing::{debug, error, info, warn};

use crate::{
    db::{
        fields::{
            AuthUserFields, ColumnCounter, ContentFields as CF, MediaFields, StrCompare,
            TSLanguage, Table,
        },
        format_sql,
        models::{AuthUser, Media, TSConfig},
        queries::{QueryObj, RespondObj, WhereBuilder},
        serialize::{AuthUserSerializer, ContentSerializer},
    },
    utils::errors::ServiceError,
};

#[cfg(debug_assertions)]
pub async fn dev_migrate(pool: &PgPool) -> Result<(), ServiceError> {
    let query: QueryObj<MediaFields> = QueryObj::default();

    let auth_resp = select_auth_user(pool, QueryObj::default()).await?;
    let media_resp = select_record::<MediaFields, Media>(pool, &Table::Media, query).await?;

    if auth_resp.results.is_empty() {
        let user = AuthUser::new(
            "admin@example.org".to_string(),
            "admin".to_string(),
            "Ad".to_string(),
            "Min".to_string(),
            "admin".to_string(),
            1,
        );

        insert_record(pool, &Table::AuthUsers, &user).await?;
    }

    if media_resp.results.is_empty() {
        let migrations_path = Path::new("../migrations_dev");
        let mut rd = fs::read_dir(migrations_path).await?;
        let mut migrations = Vec::new();
        while let Some(entry) = rd.next_entry().await? {
            if entry
                .path()
                .extension()
                .map(|ext| ext == "sql")
                .unwrap_or(false)
            {
                migrations.push(entry);
            }
        }

        migrations.sort_by_key(fs::DirEntry::path);

        for entry in migrations {
            use sqlx::Executor;

            let path = entry.path();
            let sql = fs::read_to_string(&path).await?;
            info!("Executing dev migration: {:?}", path.file_name().unwrap());

            pool.execute(&*sql).await?;

            // sqlx::query(&sql).execute(pool).await?;
        }
    }

    Ok(())
}

pub async fn db_migrate(pool: &PgPool) -> Result<(), ServiceError> {
    sqlx::migrate!("../migrations").run(pool).await?;

    #[cfg(debug_assertions)]
    dev_migrate(pool).await?;

    Ok(())
}

pub async fn select_ts_language(pool: &PgPool) -> Result<RespondObj<TSConfig>, ServiceError> {
    const QUERY: &str =
        "select cfgname, count(*) OVER() AS total_count from pg_catalog.pg_ts_config;";
    let query_obj: QueryObj<TSLanguage> = QueryObj {
        limit: 200,
        ..Default::default()
    };

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(QUERY).bright_black());

    let data: Vec<TSConfig> = sqlx::query_as(QUERY).fetch_all(pool).await?;

    Ok(RespondObj::new(&query_obj, data))
}

pub async fn select_auth_user(
    pool: &PgPool,
    query_obj: QueryObj<AuthUserFields>,
) -> Result<RespondObj<AuthUserSerializer>, ServiceError> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut separated = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            AuthUserFields::Role => separated.push("(r.id, r.name) AS \"auth_role\""),
            _ => separated.push(format!("u.{f}")),
        };
    }

    separated.push("count(*) OVER() AS total_count");

    separated.push_unseparated(" ");
    query_builder.push("FROM auth_users u ");

    if query_obj.fields.contains(&AuthUserFields::Role) {
        query_builder.push("LEFT JOIN auth_roles r ON r.id = u.role_id");
    }

    let mut where_chain = WhereBuilder::new(query_builder);

    if let Some(id) = &query_obj.search_id {
        where_chain.push_and_bind(None, "u.id = ", id, None);
    }

    if let Some(after) = &query_obj.created_after {
        where_chain.push_and_bind(None, "u.created_at >= ", after, None);
    }

    if let Some(before) = &query_obj.created_before {
        where_chain.push_and_bind(None, "u.created_at < ", before, None);
    }

    if let Some(search) = query_obj.search.clone() {
        where_chain.push_and_bind(
            None,
            "u.username LIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );

        where_chain.push_and_bind(
            Some(" OR"),
            "u.email LIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );
    }

    query_builder = where_chain.into_inner();

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
    pool: &PgPool,
    table: &Table,
    query_obj: QueryObj<T>,
) -> Result<RespondObj<M>, ServiceError>
where
    T: Display + StrCompare + IntoEnumIterator + FromStr + Debug + ts_rs::TS,
    M: for<'a> sqlx::FromRow<'a, sqlx::postgres::PgRow> + Send + Unpin + ColumnCounter,
{
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut separated = query_builder.separated(", ");

    for f in &query_obj.fields {
        separated.push(f.to_owned());
    }

    separated.push("count(*) OVER() AS total_count");
    query_builder.push(format!(" FROM {table}"));

    let mut where_chain = WhereBuilder::new(query_builder);

    if let Some(id) = &query_obj.search_id {
        where_chain.push_and_bind(None, "id = ", id, None);
    }

    query_builder = where_chain.into_inner();

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
Content
--------------------------------------- */

pub async fn select_content(
    pool: &PgPool,
    query_obj: &QueryObj<CF>,
) -> Result<RespondObj<ContentSerializer>, ServiceError> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut sep = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            CF::Author => sep.push(format!("(u.id, u.first_name, u.last_name) AS {f}")),
            CF::Categories => sep.push(format!("COALESCE(cats.data, ARRAY[]::record[]) AS {f}")),
            CF::Tags => sep.push(format!("COALESCE(tags.data, ARRAY[]::record[]) AS {f}")),
            CF::Attributes => sep.push(format!("COALESCE(att.data, ARRAY[]::record[]) AS {f}")),
            CF::Blocks => sep.push(format!("COALESCE(blocks.data, '[]') AS {f}")),
            CF::Body => sep.push("ce.text".to_string()),
            CF::Locale => sep.push(format!("l.code as {f}")),
            CF::Media => sep.push(format!("COALESCE(media_data.media, '[]') AS {f}")),
            _ => sep.push(format!("ce.{f}")),
        };
    }

    sep.push("count(*) OVER() AS total_count");
    sep.push_unseparated(" ");
    query_builder.push("FROM content_entries ce ");
    query_builder.push("JOIN content_types ct ON ct.id = ce.type_id ");

    if query_obj.fields.contains(&CF::Author) {
        query_builder.push("LEFT JOIN auth_users u ON u.id = ce.created_by ");
    }

    if query_obj.fields.contains(&CF::Categories) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT ARRAY_AGG(
                    (c.id, c.name, c.slug)
                ) AS data
                FROM content_categories c
                JOIN content_entry_categories cc ON cc.category_id = c.id
                WHERE cc.entry_id = ce.id
            ) AS cats ON TRUE "#,
        );
    }

    if query_obj.fields.contains(&CF::Tags) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT ARRAY_AGG(
                    (t.id, t.name, t.slug)
                ) AS data
                FROM content_tags t
                JOIN content_entry_tags ct ON ct.tag_id = t.id
                WHERE ct.entry_id = ce.id
            ) AS tags ON TRUE "#,
        );
    }

    if query_obj.fields.contains(&CF::Attributes) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT ARRAY_AGG(
                    (a.id, a.name, a.value)
                ) AS data
                FROM content_attributes a
                WHERE a.entry_id = ce.id
            ) AS att ON TRUE "#,
        );
    }

    if query_obj.fields.contains(&CF::Blocks) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', bl.id,
                        'type', bl.type,
                        'data', bl.data
                    )
                    ORDER BY bl.order_index
                ) AS data
                FROM content_blocks bl
                WHERE bl.entry_id = ce.id
            ) AS blocks ON TRUE "#,
        );
    }

    if query_obj.fields.contains(&CF::Locale) || query_obj.search_locale.is_some() {
        query_builder.push("JOIN locales l ON l.id = ce.locale_id ");
    }

    if query_obj.fields.contains(&CF::Media) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT json_agg(
                    json_build_object(
                        'id', m.id,
                        'alt', m.alt,
                        'filename', m.filename,
                        'path', m.path,
                        'type', m.type,
                        'ast_line', cm.ast_line,
                        'start_offset', cm.start_offset,
                        'end_offset', cm.end_offset,
                        'variants', COALESCE(
                            (
                                SELECT json_agg(
                                    json_build_object(
                                        'id', mv.id,
                                        'resolution', mv.resolution,
                                        'format', mv.format,
                                        'filename', mv.filename
                                    )
                                )
                                FROM media_variants mv
                                WHERE mv.media_id = m.id
                            ),
                            '[]'
                        )
                    )
                ) AS media
                FROM content_media cm
                JOIN media m ON m.id = cm.media_id
                WHERE cm.entry_id = ce.id
            ) AS media_data ON TRUE "#,
        );
    }

    let mut where_chain = WhereBuilder::new(query_builder);

    if let Some(id) = &query_obj.search_id {
        where_chain.push_and_bind(None, "ce.id = ", id, None);
    }

    if let Some(locale) = &query_obj.search_locale {
        where_chain.push_and_bind(None, "l.code = ", locale, None);
    }

    if let Some(after) = &query_obj.created_after {
        where_chain.push_and_bind(None, "ce.created_at >= ", after, None);
    }

    if let Some(before) = &query_obj.created_before {
        where_chain.push_and_bind(None, "ce.created_at < ", before, None);
    }

    if let Some(ts) = &query_obj.type_slug {
        where_chain.push_and_bind(None, "ct.slug = ", ts.to_string(), None);
    }

    if let Some(slug) = &query_obj.search_slug {
        where_chain.push_and_bind(None, "ce.slug = ", slug, None);
    }

    if let Some(status) = &query_obj.search_status {
        where_chain.push_and_bind(None, "ce.status = ", status, None);
    }

    if let Some(search) = query_obj.search.clone() {
        where_chain.push_and_bind(
            None,
            "title LIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );

        // TODO: add full text search
    }

    // take builder back from where_chain
    query_builder = where_chain.into_inner();

    if query_obj
        .fields
        .iter()
        .any(|f| query_obj.ordering.contains(&f.to_string()))
    {
        let ordering = query_obj
            .ordering
            .split(',')
            .map(|item| {
                let item = item.trim();
                if item.contains("author") {
                    item.replace("author", "u.last_name")
                } else if item.contains("locale") {
                    item.replace("locale", "l.code")
                } else {
                    format!("ce.{item}")
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        query_builder.push(format!(" ORDER BY {}", ordering));
    }

    query_builder.push(format!(
        " LIMIT {} OFFSET {}",
        query_obj.limit, query_obj.offset
    ));

    let query = query_builder.build_query_as::<ContentSerializer>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()).bright_black());

    let data: Vec<ContentSerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(query_obj, data))
}
