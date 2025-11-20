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
use rand::{Rng, distr::Alphanumeric};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Postgres, QueryBuilder, postgres::PgPool};
use strum::IntoEnumIterator;
use tokio::task;
use tracing::{debug, error, warn};

use crate::{
    db::{
        fields::{
            AuthUserFields, ColumnCounter, ContentAuthorFields, ContentCategoryFields as CCF,
            ContentFields as CF, MediaFields, StrCompare, TSLanguage, Table,
        },
        models::{Configuration, TSConfig},
        queries::{QueryObj, RespondObj, WhereBuilder},
        serialize::{
            AuthUserSerializer, AuthorSerializer, ContentCategorySerializer, ContentSerializer,
            MediaSerializer,
        },
    },
    utils::errors::ServiceError,
};

#[cfg(debug_assertions)]
use {sqlx::Execute, std::env, tokio::fs, tracing::info};

#[cfg(debug_assertions)]
use crate::db::{
    format_sql,
    models::{AuthUser, Media},
};

#[cfg(debug_assertions)]
pub async fn dev_migrate(pool: &PgPool) -> Result<(), ServiceError> {
    let query: QueryObj<MediaFields> = QueryObj {
        limit: 1,
        fields: vec![MediaFields::ID],
        ..Default::default()
    };

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

        insert_record::<AuthUser, i32>(pool, &Table::AuthUsers, &user).await?;
    }

    if media_resp.results.is_empty() {
        let migrations_path = env::current_dir()?.join("../migrations_dev");
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
        }
    }

    Ok(())
}

pub async fn db_migrate(pool: &PgPool) -> Result<(), ServiceError> {
    sqlx::migrate!("../migrations").run(pool).await?;

    if select_configuration(pool).await.is_err() {
        let secret: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(80)
            .map(char::from)
            .collect();

        const QUERY: &str = "INSERT INTO configuration(jwt_secret, image_extensions, image_resolutions) VALUES($1, ARRAY['jpg', 'avif', 'webp'], ARRAY[1024, 480]);";

        sqlx::query(QUERY).bind(secret).execute(pool).await?;
    }

    #[cfg(debug_assertions)]
    dev_migrate(pool).await?;

    Ok(())
}

pub async fn select_configuration(pool: &PgPool) -> Result<Configuration, ServiceError> {
    const QUERY: &str = "select * from configuration;";

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(QUERY).bright_black());

    let data: Configuration = sqlx::query_as(QUERY).fetch_one(pool).await?;

    Ok(data)
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
            "u.username ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );

        where_chain.push_and_bind(
            Some(" OR"),
            "u.email ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );
    }

    query_builder = where_chain.into_inner();

    let ordering: Vec<String> = query_obj
        .ordering
        .split(',')
        .filter_map(|part| {
            let mut split = part.split_whitespace();
            let column = split.next()?;
            let direction = split.next().unwrap_or("ASC").to_uppercase();

            if query_obj.fields.iter().any(|f| f.to_string() == column)
                && (direction == "ASC" || direction == "DESC")
            {
                Some(format!("{column} {direction}"))
            } else {
                None
            }
        })
        .collect();

    if !ordering.is_empty() {
        query_builder.push(" ORDER BY ");
        query_builder.push(ordering.join(", "));
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

pub async fn insert_record<T, R>(pool: &PgPool, table: &Table, data: &T) -> Result<R, ServiceError>
where
    T: Serialize,
    R: sqlx::Type<Postgres> + Send + Unpin + for<'r> sqlx::Decode<'r, Postgres>,
{
    let value = serde_json::to_value(data)?;

    let obj = match value.as_object() {
        Some(map) => map.clone(),
        None => return Err(ServiceError::NoContent),
    };

    let type_ignore = [
        "id",
        "created_at",
        "updated_at",
        "last_login",
        "total_count",
    ];

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

    let ordering: Vec<String> = query_obj
        .ordering
        .split(',')
        .filter_map(|part| {
            let mut split = part.split_whitespace();
            let column = split.next()?;
            let direction = split.next().unwrap_or("ASC").to_uppercase();

            if query_obj.fields.iter().any(|f| f.to_string() == column)
                && (direction == "ASC" || direction == "DESC")
            {
                Some(format!("{column} {direction}"))
            } else {
                None
            }
        })
        .collect();

    if !ordering.is_empty() {
        query_builder.push(" ORDER BY ");
        query_builder.push(ordering.join(", "));
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

pub async fn select_content_author(
    pool: &PgPool,
    query_obj: QueryObj<ContentAuthorFields>,
) -> Result<RespondObj<AuthorSerializer>, ServiceError> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut separated = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            ContentAuthorFields::Media => {
                separated.push("COALESCE(media.data, '{}'::json) AS \"media\"")
            }
            ContentAuthorFields::MediaID => {
                if !query_obj.fields.contains(&ContentAuthorFields::Media) {
                    separated.push(format!("ca.{f}"));
                }
                continue;
            }
            _ => separated.push(format!("ca.{f}")),
        };
    }

    separated.push("count(*) OVER() AS total_count");

    separated.push_unseparated(" ");
    query_builder.push("FROM content_authors ca ");

    if query_obj.fields.contains(&ContentAuthorFields::Media) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT json_build_object(
                    'id', m.id,
                    'alt', m.alt,
                    'path', m.path,
                    'variants', COALESCE(
                        (
                            SELECT json_agg(
                                json_build_object(
                                    'id', mv.id,
                                    'width', mv.width,
                                    'height', mv.height,
                                    'filename', mv.filename
                                )
                            )
                            FROM media_variants mv
                            WHERE mv.media_id = m.id
                        ),
                        '[]'
                    )
                ) AS data
                FROM media m
                WHERE m.id = ca.media_id
            ) AS media ON TRUE "#,
        );
    }

    let mut where_chain = WhereBuilder::new(query_builder);

    if let Some(id) = &query_obj.search_id {
        where_chain.push_and_bind(None, "ca.id = ", id, None);
    }

    if let Some(after) = &query_obj.created_after {
        where_chain.push_and_bind(None, "ca.created_at >= ", after, None);
    }

    if let Some(before) = &query_obj.created_before {
        where_chain.push_and_bind(None, "ca.created_at < ", before, None);
    }

    if let Some(search) = query_obj.search.clone() {
        where_chain.push_and_bind(
            None,
            "(ca.first_name ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );

        where_chain.push_and_bind(
            Some(" OR"),
            "ca.last_name ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%'))"),
        );
    }

    query_builder = where_chain.into_inner();

    let ordering: Vec<String> = query_obj
        .ordering
        .split(',')
        .filter_map(|part| {
            let mut split = part.split_whitespace();
            let column = split.next()?;
            let direction = split.next().unwrap_or("ASC").to_uppercase();

            if query_obj.fields.iter().any(|f| f.to_string() == column)
                && (direction == "ASC" || direction == "DESC")
            {
                Some(format!("{column} {direction}"))
            } else {
                None
            }
        })
        .collect();

    if !ordering.is_empty() {
        query_builder.push(" ORDER BY ");
        query_builder.push(ordering.join(", "));
    }

    query_builder.push(format!(
        " LIMIT {} OFFSET {}",
        query_obj.limit, query_obj.offset
    ));

    let query = query_builder.build_query_as::<AuthorSerializer>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()).bright_black());

    let data: Vec<AuthorSerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(&query_obj, data))
}

pub async fn select_categories(
    pool: &PgPool,
    query_obj: &QueryObj<CCF>,
) -> Result<RespondObj<ContentCategorySerializer>, ServiceError> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut sep = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            CCF::Media => sep.push("COALESCE(media.data, NULL) AS \"media\""),
            CCF::GroupMembers => sep.push(format!("COALESCE(group_members.data, '[]') AS {f}")),
            _ => sep.push(format!("cc.{f}")),
        };
    }

    sep.push("count(*) OVER() AS total_count");
    sep.push_unseparated(" ");
    query_builder.push("FROM content_categories cc ");

    if query_obj.fields.contains(&CCF::LocaleID) || query_obj.search_locale.is_some() {
        query_builder.push("LEFT JOIN locales l ON l.id = cc.locale_id ");
    }

    if query_obj.fields.contains(&CCF::Media) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT json_build_object(
                    'id', m.id,
                    'alt', m.alt,
                    'path', m.path,
                    'variants', COALESCE(
                        (
                            SELECT json_agg(
                                json_build_object(
                                    'id', mv.id,
                                    'width', mv.width,
                                    'height', mv.height,
                                    'filename', mv.filename
                                )
                            )
                            FROM media_variants mv
                            WHERE mv.media_id = m.id
                        ),
                        '[]'
                    )
                ) AS data
                FROM media m
                WHERE m.id = cc.media_id
            ) AS media ON TRUE "#,
        );
    }

    if query_obj.fields.contains(&CCF::GroupMembers) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', ge.id,
                        'locale_id', ge.locale_id
                    )
                ) AS data
                FROM content_entries ge
                WHERE ge.group_id = cc.group_id
                  AND ge.id != cc.id
            ) AS group_members ON TRUE "#,
        );
    }

    let mut where_chain = WhereBuilder::new(query_builder);

    if let Some(id) = &query_obj.search_id {
        where_chain.push_and_bind(None, "cc.id = ", id, None);
    }

    if let Some(locale) = &query_obj.search_locale {
        where_chain.push_and_bind(None, "l.code = ", locale, None);
    }

    if let Some(slug) = &query_obj.search_slug {
        where_chain.push_and_bind(None, "cc.slug = ", slug, None);
    }

    if let Some(status) = &query_obj.search_status {
        where_chain.push_and_bind(None, "cc.status = ", status, None);
    }

    if let Some(id) = &query_obj.group_id {
        where_chain.push_and_bind(None, "cc.group_id = ", id, None);
    }

    if let Some(search) = query_obj.search.clone() {
        where_chain.push_and_bind(
            None,
            "cc.name ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );
    }

    // take builder back from where_chain
    query_builder = where_chain.into_inner();

    let ordering = query_obj
        .ordering
        .split(',')
        .filter_map(|item| {
            let item = item.trim();
            if CCF::iter().any(|f| item.contains(&f.to_string())) {
                Some(format!("cc.{item}"))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    if !ordering.is_empty() {
        query_builder.push(format!(" ORDER BY {}", ordering));
    }

    query_builder.push(format!(
        " LIMIT {} OFFSET {}",
        query_obj.limit, query_obj.offset
    ));

    let query = query_builder.build_query_as::<ContentCategorySerializer>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()).bright_black());

    let data: Vec<ContentCategorySerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(query_obj, data))
}

pub async fn select_content(
    pool: &PgPool,
    query_obj: &QueryObj<CF>,
) -> Result<RespondObj<ContentSerializer>, ServiceError> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut sep = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            CF::Author => sep.push(format!(
                "(ca.id, ca.first_name, ca.last_name, ca.media_id) AS {f}"
            )),
            CF::Categories => sep.push(format!("COALESCE(cats.data, ARRAY[]::record[]) AS {f}")),
            CF::Tags => sep.push(format!("COALESCE(tags.data, ARRAY[]::record[]) AS {f}")),
            CF::Meta => sep.push(format!("(cm.data, cm.start_time, cm.end_time) AS {f}")),
            CF::Blocks => sep.push(format!("COALESCE(blocks.data, '[]') AS {f}")),
            CF::Body => sep.push("ce.text".to_string()),
            CF::GroupMembers => sep.push(format!("COALESCE(group_members.data, '[]') AS {f}")),
            CF::Media => sep.push(format!("COALESCE(media_data.media, '[]') AS {f}")),
            _ => sep.push(format!("ce.{f}")),
        };
    }

    sep.push("count(*) OVER() AS total_count");
    sep.push_unseparated(" ");
    query_builder.push("FROM content_entries ce ");

    if query_obj.type_slug.is_some() {
        query_builder.push("JOIN content_types ct ON ct.id = ce.type_id ");
    }

    if query_obj.fields.contains(&CF::Author)
        || query_obj.author.is_some()
        || query_obj.search.is_some()
    {
        query_builder.push(
            r#"LEFT JOIN content_entry_authors cea ON cea.entry_id = ce.id
            LEFT JOIN content_authors ca ON ca.id = cea.author_id "#,
        );
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
                JOIN content_entry_tags cet ON cet.tag_id = t.id
                WHERE cet.entry_id = ce.id
            ) AS tags ON TRUE "#,
        );
    }

    if query_obj.fields.contains(&CF::Meta)
        || query_obj.start_time.is_some()
        || query_obj.end_time.is_some()
    {
        query_builder.push("LEFT JOIN content_meta cm ON cm.entry_id = ce.id ");
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

    if query_obj.fields.contains(&CF::GroupMembers) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', ge.id,
                        'locale_id', ge.locale_id
                    )
                ) AS data
                FROM content_entries ge
                WHERE ge.group_id = ce.group_id
                  AND ge.id != ce.id
            ) AS group_members ON TRUE "#,
        );
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
                                        'width', mv.width,
                                        'height', mv.height,
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

    if let Some(id) = &query_obj.search_locale {
        where_chain.push_and_bind(None, "ce.locale_id = ", id, None);
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

    if let Some(id) = &query_obj.type_id {
        where_chain.push_and_bind(None, "ce.type_id = ", id, None);
    }

    if let Some(slug) = &query_obj.search_slug {
        where_chain.push_and_bind(None, "ce.slug = ", slug, None);
    }

    if let Some(status) = &query_obj.search_status {
        where_chain.push_and_bind(None, "ce.status = ", status, None);
    }

    if let Some(id) = &query_obj.author {
        where_chain.push_and_bind(None, "ca.id = ", id, None);
    }

    if let Some(id) = &query_obj.group_id {
        where_chain.push_and_bind(None, "ce.group_id = ", id, None);
    }

    if let Some(start) = &query_obj.start_time {
        where_chain.push_and_bind(None, "cm.start_time >= ", start, None);
    }

    if let Some(end) = &query_obj.end_time {
        where_chain.push_and_bind(None, "cm.end_time <= ", end, None);
    }

    if let Some(search) = query_obj.search.clone() {
        where_chain.push_and_bind(
            None,
            "(ce.title ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );

        where_chain.push_and_bind(
            Some("OR"),
            "ca.first_name ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );

        where_chain.push_and_bind(
            Some("OR"),
            "ca.last_name ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%'))"),
        );

        // TODO: add full text search
    }

    // take builder back from where_chain
    query_builder = where_chain.into_inner();

    let ordering = query_obj
        .ordering
        .split(',')
        .filter_map(|item| {
            let item = item.trim();
            if item.contains("author") {
                Some(item.replace("author", "u.last_name"))
            } else if item.contains("locale") {
                Some(item.replace("locale", "l.code"))
            } else if item.contains("start_time") {
                Some(item.replace("start_time", "m.start_time"))
            } else if item.contains("end_time") {
                Some(item.replace("end_time", "m.end_time"))
            } else if CF::iter().any(|f| item.contains(&f.to_string())) {
                Some(format!("ce.{item}"))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    if !ordering.is_empty() {
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

pub async fn select_media(
    pool: &PgPool,
    query_obj: &QueryObj<MediaFields>,
) -> Result<RespondObj<MediaSerializer>, ServiceError> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut sep = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            MediaFields::MediaVariants => sep.push("COALESCE(variants.data, NULL) AS \"variants\""),
            _ => sep.push(format!("m.{f}")),
        };
    }

    sep.push("count(*) OVER() AS total_count");
    sep.push_unseparated(" ");
    query_builder.push("FROM media m ");
    if query_obj.fields.contains(&MediaFields::MediaVariants) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT json_agg(
                    json_build_object(
                        'id', mv.id,
                        'width', mv.width,
                        'height', mv.height,
                        'filename', mv.filename
                    )
                ) AS data
                FROM media_variants mv
                WHERE mv.media_id = m.id
            ) AS variants ON TRUE "#,
        );
    }

    let mut where_chain = WhereBuilder::new(query_builder);

    if let Some(id) = &query_obj.search_id {
        where_chain.push_and_bind(None, "m.id = ", id, None);
    }

    if let Some(search) = query_obj.search.clone() {
        where_chain.push_and_bind(
            None,
            "m.filename ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );
    }

    if !query_obj.media_type.is_empty() {
        let array: Vec<String> = query_obj
            .media_type
            .iter()
            .map(|t| format!("{t}/%"))
            .collect();
        where_chain.push_and_bind(None, "m.type LIKE ANY(", array, Some(")"));
    }

    // take builder back from where_chain
    query_builder = where_chain.into_inner();

    let ordering = query_obj
        .ordering
        .split(',')
        .filter_map(|item| {
            let item = item.trim();
            if MediaFields::iter().any(|f| item.contains(&f.to_string())) {
                Some(format!("m.{item}"))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    if !ordering.is_empty() {
        query_builder.push(format!(" ORDER BY {}", ordering));
    }

    query_builder.push(format!(
        " LIMIT {} OFFSET {}",
        query_obj.limit, query_obj.offset
    ));

    let query = query_builder.build_query_as::<MediaSerializer>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()).bright_black());

    let data: Vec<MediaSerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(query_obj, data))
}
