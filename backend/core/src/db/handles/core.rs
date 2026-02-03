use std::{
    fmt::{Debug, Display},
    str::FromStr,
    string::ToString,
};

use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Postgres, QueryBuilder, postgres::PgPool};
use strum::IntoEnumIterator;
use tracing::{debug, error, warn};

use crate::db::{
    fields::{ColumnCounter, StrCompare, Table},
    queries::{QueryObj, RespondObj, WhereBuilder},
};
use crate::utils::errors::NurError;

#[cfg(debug_assertions)]
use crate::db::format_sql;
#[cfg(debug_assertions)]
use sqlx::Execute;

pub async fn delete_record(pool: &PgPool, table: &Table, id: i32) -> Result<(), NurError> {
    let mut qb = QueryBuilder::<Postgres>::new(format!("DELETE FROM {table} WHERE id = "));
    qb.push_bind(id);

    let query = qb.build();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()));

    let rows_affected = query.execute(pool).await?.rows_affected();

    if rows_affected == 0 {
        let msg = format!("No record with id={id} found in {table}");
        warn!("{msg}");
        return Err(NurError::UnprocessableEntity(msg));
    }

    debug!("Deleted record with id={id} from {table}");

    Ok(())
}

pub async fn select_record<T, M>(
    pool: &PgPool,
    table: &Table,
    query_obj: QueryObj<T>,
) -> Result<RespondObj<M>, NurError>
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
    debug!("{}", format_sql(query.sql()));

    let data: Vec<M> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(&query_obj, data))
}

pub async fn insert_record<T, R>(pool: &PgPool, table: &Table, data: &T) -> Result<R, NurError>
where
    T: Serialize,
    R: sqlx::Type<Postgres> + Send + Unpin + for<'r> sqlx::Decode<'r, Postgres>,
{
    let value = serde_json::to_value(data)?;
    let return_field = match table {
        Table::ContentEntryTags => "tag_id",
        Table::ContentEntryAuthors => "author_id",
        _ => "id",
    };

    let obj = match value.as_object() {
        Some(map) => map.clone(),
        None => return Err(NurError::NoContent),
    };

    let type_ignore = ["id", "last_login", "total_count"];
    let type_time = ["created_at", "updated_at", "start_time", "end_time"];

    let mut keys = Vec::new();
    let mut qb = QueryBuilder::<Postgres>::new(format!("INSERT INTO {table} ("));

    for key in obj.keys() {
        if type_ignore.contains(&key.as_str()) {
            continue;
        }
        keys.push(key.clone());
    }

    if keys.is_empty() {
        return Err(NurError::NoContent);
    }

    qb.push(keys.join(", "));
    qb.push(") VALUES (");

    let mut separated = qb.separated(", ");
    for key in &keys {
        let val = &obj[key];

        match val {
            Value::Array(a) => {
                if a.iter().all(|v| matches!(v, Value::String(_))) {
                    let values: Vec<&str> = a
                        .iter()
                        .map(|v| match v {
                            Value::String(s) => s.as_str(),
                            _ => unreachable!(),
                        })
                        .collect();

                    separated.push_bind(values);
                } else if a.iter().all(|v| matches!(v, Value::Number(_))) {
                    let values: Vec<i64> = a
                        .iter()
                        .filter_map(|v| match v {
                            Value::Number(n) => n.as_i64(),
                            _ => unreachable!(),
                        })
                        .collect();

                    separated.push_bind(values);
                } else {
                    return Err(NurError::InvalidInput);
                }
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
                if type_time.contains(&key.as_str()) {
                    let dt: DateTime<Utc> = match DateTime::parse_from_rfc3339(s) {
                        Ok(t) => t.into(),
                        Err(_) => Utc::now(),
                    };

                    separated.push_bind(dt);
                } else if key == "password" {
                    use argon2::{
                        Argon2, PasswordHasher,
                        password_hash::{SaltString, rand_core::OsRng},
                    };
                    let pw = s.clone();
                    let password_hash =
                        tokio::task::spawn_blocking(move || -> Result<String, NurError> {
                            let salt = SaltString::generate(&mut OsRng);
                            let hash = Argon2::default()
                                .hash_password(pw.as_bytes(), &salt)
                                .map_err(|_| NurError::InternalServerError)?;

                            Ok(hash.to_string())
                        })
                        .await??;

                    separated.push_bind(password_hash);
                } else {
                    separated.push_bind(s);
                }
            }
            Value::Object(o) => {
                separated.push_bind(serde_json::Value::Object(o.clone()));
            }
        }
    }

    qb.push(format!(") RETURNING {return_field}"));

    let query = qb.build_query_scalar();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()));

    let id = query.fetch_one(pool).await?;

    Ok(id)
}

pub async fn update_record<T, I>(
    pool: &PgPool,
    table: &Table,
    id: I,
    data: &T,
) -> Result<(), NurError>
where
    T: Serialize,
    I: for<'q> sqlx::Encode<'q, Postgres> + sqlx::Type<Postgres> + Send,
{
    let value = serde_json::to_value(data)?;

    let obj = match value.as_object() {
        Some(map) => map.clone(),
        None => return Ok(()),
    };

    let type_ignore = ["id", "created_at", "nodes", "meta"];
    let type_time = ["updated_at", "last_login"];

    let mut qb = QueryBuilder::<Postgres>::new(format!("UPDATE {table} SET "));
    let mut separated = qb.separated(", ");
    let mut any_field = false;

    for (key, val) in obj {
        if type_ignore.contains(&key.as_str()) {
            continue;
        }
        any_field = true;
        separated.push(format!("{key} = "));

        match val {
            Value::Array(a) => {
                if a.iter().all(Value::is_string) {
                    let vec: Vec<String> = a
                        .iter()
                        .filter_map(|v| v.as_str().map(ToString::to_string))
                        .collect();
                    separated.push_bind_unseparated(vec);
                } else if a.iter().all(Value::is_number) {
                    let vec: Vec<i32> = a
                        .iter()
                        .filter_map(|v| v.as_i64().map(|n| n as i32))
                        .collect();
                    separated.push_bind_unseparated(vec);
                } else {
                    separated.push_bind_unseparated(a);
                }
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
                        tokio::task::spawn_blocking(move || -> Result<String, NurError> {
                            let salt = SaltString::generate(&mut OsRng);
                            let hash = Argon2::default()
                                .hash_password(pw.as_bytes(), &salt)
                                .map_err(|_| NurError::InternalServerError)?;

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
            Value::Null => {
                if key.contains("_id") {
                    separated.push_bind_unseparated(None::<i32>);
                } else {
                    separated.push_bind_unseparated(None::<String>);
                }
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
    debug!("{}", format_sql(query.sql()));

    query.execute(pool).await?;

    Ok(())
}
