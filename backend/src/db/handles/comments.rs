use sqlx::{Postgres, QueryBuilder, postgres::PgPool};
use strum::IntoEnumIterator;

#[cfg(debug_assertions)]
use sqlx::Execute;
#[cfg(debug_assertions)]
use tracing::debug;

use crate::db::{
    fields::CommentFields,
    models::Comment,
    queries::{QueryObj, RespondObj, WhereBuilder},
};
use crate::utils::errors::ServiceError;

#[cfg(debug_assertions)]
use crate::db::format_sql;

pub async fn select_comments(
    pool: &PgPool,
    query_obj: &QueryObj<CommentFields>,
) -> Result<RespondObj<Comment>, ServiceError> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut sep = query_builder.separated(", ");

    for f in &query_obj.fields {
        sep.push(f.to_string());
    }

    sep.push("count(*) OVER() AS total_count");
    sep.push_unseparated(" ");
    query_builder.push("FROM comments ");

    let mut where_chain = WhereBuilder::new(query_builder);

    if let Some(id) = &query_obj.search_id {
        where_chain.push_and_bind(None, "id = ", id, None);
    }

    if let Some(id) = &query_obj.entry_id {
        where_chain.push_and_bind(None, "entry_id = ", id, None);
    }

    if let Some(status) = &query_obj.search_status {
        where_chain.push_and_bind(None, "status = ", status, None);
    }

    if let Some(search) = &query_obj.search {
        where_chain.push_and_bind(
            None,
            "author_name ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );
        where_chain.push_and_bind(
            Some("OR"),
            "author_email ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );
        where_chain.push_and_bind(
            Some("OR"),
            "text ILIKE CONCAT('%', ",
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
            if CommentFields::iter().any(|f| item.contains(&f.to_string())) {
                Some(item.to_string())
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

    let query = query_builder.build_query_as::<Comment>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()));

    let data: Vec<Comment> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(query_obj, data))
}

pub async fn insert_comment(pool: &PgPool, c: &Comment) -> Result<i64, ServiceError> {
    let entry_id = c.entry_id.ok_or(ServiceError::InvalidInput)?;
    let text = c.text.as_deref().ok_or(ServiceError::InvalidInput)?;
    let status = c.status.as_deref().unwrap_or("pending");

    const QUERY: &str = r#"
        INSERT INTO comments (entry_id, author_name, author_email, status, text)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
    "#;

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(QUERY));

    let id = sqlx::query_scalar(QUERY)
        .bind(entry_id)
        .bind(&c.author_name)
        .bind(&c.author_email)
        .bind(status)
        .bind(text)
        .fetch_one(pool)
        .await?;

    Ok(id)
}
