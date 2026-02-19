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
use crate::utils::errors::NurError;

#[cfg(debug_assertions)]
use crate::db::format_sql;

pub async fn select_comments(
    pool: &PgPool,
    query_obj: &QueryObj<CommentFields>,
) -> Result<RespondObj<Comment>, NurError> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut sep = qb.separated(", ");

    for f in &query_obj.fields {
        if *f == CommentFields::Entry {
            sep.push("(e.id, e.title, t.slug, e.slug) AS entry".to_string());
        } else {
            sep.push(format!("c.{f}"));
        }
    }

    sep.push("count(*) OVER() AS total_count");
    sep.push_unseparated(" ");
    qb.push("FROM comments c ");

    if query_obj.fields.contains(&CommentFields::Entry) {
        qb.push(
            "LEFT JOIN content_entries e ON e.id = c.entry_id
            LEFT JOIN content_types t ON t.id = e.type_id ",
        );
    }

    let mut where_chain = WhereBuilder::new(qb);

    if let Some(id) = &query_obj.search_id {
        where_chain.push_and_bind(None, "c.id = ", id, None);
    }

    if let Some(id) = &query_obj.entry_id {
        where_chain.push_and_bind(None, "c.entry_id = ", id, None);
    }

    if let Some(status) = &query_obj.search_status {
        where_chain.push_and_bind(None, "c.status = ", status, None);
    }

    if let Some(slug) = &query_obj.search_slug {
        where_chain.push_and_bind(None, "e.slug = ", slug, None);
    }

    if let Some(search) = &query_obj.search {
        where_chain.push_and_bind(
            None,
            "c.author_name ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );
        where_chain.push_and_bind(
            Some("OR"),
            "c.author_email ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );
        where_chain.push_and_bind(
            Some("OR"),
            "c.text ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );
    }

    // take builder back from where_chain
    qb = where_chain.into_inner();

    let ordering = query_obj
        .ordering
        .split(',')
        .filter_map(|item| {
            let item = item.trim();
            if CommentFields::iter().any(|f| item.contains(&f.to_string())) {
                Some(format!("c.{item}"))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    if !ordering.is_empty() {
        qb.push(format!(" ORDER BY {}", ordering));
    }

    qb.push(format!(
        " LIMIT {} OFFSET {}",
        query_obj.limit, query_obj.offset
    ));

    let query = qb.build_query_as::<Comment>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()));

    let data: Vec<Comment> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(query_obj, data))
}

pub async fn insert_comment(pool: &PgPool, c: &Comment) -> Result<i64, NurError> {
    let entry_id = c.entry_id.ok_or(NurError::InvalidInput)?;
    let text = c.text.as_deref().ok_or(NurError::InvalidInput)?;
    let status = c.status.as_deref().unwrap_or("pending");
    let mut qb = QueryBuilder::<Postgres>::new("INSERT INTO comments (");
    let mut keys = vec!["entry_id", "text", "status"];

    if c.parent_id.is_some() {
        keys.push("parent_id");
    }

    if c.author_email.is_some() {
        keys.push("author_email");
    }

    if c.author_name.is_some() {
        keys.push("author_name");
    }

    if c.user_id.is_some() && c.author_email.is_none() {
        keys.push("user_id");
    }

    if c.user_id.is_some() && c.created_at.is_some() {
        keys.push("created_at");
    }

    if c.user_id.is_some() && c.updated_at.is_some() {
        keys.push("updated_at");
    }

    qb.push(keys.join(", "));
    qb.push(") VALUES (");

    let mut separated = qb.separated(", ");
    separated.push_bind(entry_id);
    separated.push_bind(text);
    separated.push_bind(status);

    if let Some(parent_id) = c.parent_id {
        separated.push_bind(parent_id);
    }

    if let Some(author_email) = c.author_email.as_deref() {
        separated.push_bind(author_email);
    }

    if let Some(author_name) = c.author_name.as_deref() {
        separated.push_bind(author_name);
    }

    if let Some(user_id) = c.user_id
        && c.author_email.is_none()
    {
        separated.push_bind(user_id);
    }

    if c.user_id.is_some() {
        if let Some(created_at) = c.created_at {
            separated.push_bind(created_at);
        }
        if let Some(updated_at) = c.updated_at {
            separated.push_bind(updated_at);
        }
    }

    qb.push(") RETURNING id");

    let query = qb.build_query_scalar();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()));

    let id = query.fetch_one(pool).await?;

    Ok(id)
}
