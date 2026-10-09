use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::postgres::PgPool;

use crate::{
    db::{
        fields::Table,
        handles::{normalize_entry_node_templates, sync_entry_nodes, update_record},
        models::{ContentEntry, ContentMeta},
    },
    utils::errors::NurError,
};

pub async fn find_entry_by_slug(
    pool: &PgPool,
    slug: &str,
    locale_id: i32,
    type_id: i32,
) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM content_entries WHERE slug = $1 AND locale_id = $2 AND type_id = $3",
    )
    .bind(slug)
    .bind(locale_id)
    .bind(type_id)
    .fetch_optional(pool)
    .await
}

pub async fn update_imported_entry(
    pool: &PgPool,
    entry_id: i32,
    entry: &ContentEntry,
    nodes: &[Value],
    meta: Option<&ContentMeta>,
    author_ids: &[i32],
    tag_ids: &[i32],
) -> Result<(), NurError> {
    let mut transaction = pool.begin().await?;
    let mut nodes = nodes.to_vec();

    normalize_entry_node_templates(&mut transaction, &mut nodes).await?;
    update_record(&mut *transaction, &Table::ContentEntries, entry_id, entry).await?;
    sync_entry_nodes(&mut transaction, entry_id, &nodes).await?;

    if let Some(meta) = meta {
        sqlx::query(
            "INSERT INTO content_meta (entry_id, start_time, end_time) VALUES ($1, $2, $3) \
             ON CONFLICT (entry_id) DO UPDATE SET start_time = EXCLUDED.start_time, end_time = EXCLUDED.end_time",
        )
        .bind(entry_id)
        .bind(meta.start_time)
        .bind(meta.end_time)
        .execute(&mut *transaction)
        .await?;
    }

    sqlx::query("DELETE FROM content_entry_authors WHERE entry_id = $1")
        .bind(entry_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM content_entry_tags WHERE entry_id = $1")
        .bind(entry_id)
        .execute(&mut *transaction)
        .await?;

    for author_id in author_ids {
        sqlx::query("INSERT INTO content_entry_authors (entry_id, author_id) VALUES ($1, $2)")
            .bind(entry_id)
            .bind(author_id)
            .execute(&mut *transaction)
            .await?;
    }

    for tag_id in tag_ids {
        sqlx::query("INSERT INTO content_entry_tags (entry_id, tag_id) VALUES ($1, $2)")
            .bind(entry_id)
            .bind(tag_id)
            .execute(&mut *transaction)
            .await?;
    }

    transaction.commit().await?;

    Ok(())
}

pub async fn select_node_template(
    pool: &PgPool,
    name: &str,
) -> Result<Option<(i32, Value, Value)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, data, schema FROM content_node_templates WHERE name = $1 ORDER BY id LIMIT 1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
}

pub async fn insert_data_node(
    pool: &PgPool,
    entry_id: i32,
    order_index: i32,
    name: &str,
    data: Value,
    template_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO content_nodes (entry_id, order_index, name, data, template_id) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(entry_id)
    .bind(order_index)
    .bind(name)
    .bind(data)
    .bind(template_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn insert_text_node(
    pool: &PgPool,
    entry_id: i32,
    order_index: i32,
    text: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO content_nodes (entry_id, order_index, text) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(entry_id)
    .bind(order_index)
    .bind(text)
    .fetch_one(pool)
    .await
}

pub async fn find_category(
    pool: &PgPool,
    name: &str,
    locale_id: i32,
) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM content_categories WHERE name = $1 AND locale_id = $2 LIMIT 1",
    )
    .bind(name)
    .bind(locale_id)
    .fetch_optional(pool)
    .await
}

pub async fn insert_category(
    pool: &PgPool,
    name: &str,
    slug: &str,
    locale_id: i32,
) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO content_categories (name, slug, locale_id, status) VALUES ($1, $2, $3, 'published') RETURNING id",
    )
    .bind(name)
    .bind(slug)
    .bind(locale_id)
    .fetch_one(pool)
    .await
}

pub async fn find_author(
    pool: &PgPool,
    first_name: &str,
    last_name: Option<&str>,
) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM content_authors WHERE first_name = $1 AND last_name IS NOT DISTINCT FROM $2 LIMIT 1",
    )
    .bind(first_name)
    .bind(last_name)
    .fetch_optional(pool)
    .await
}

pub async fn insert_author(
    pool: &PgPool,
    first_name: &str,
    last_name: Option<&str>,
    slug: &str,
    created_at: DateTime<Utc>,
) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO content_authors (first_name, last_name, slug, created_at, updated_at) VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(first_name)
    .bind(last_name)
    .bind(slug)
    .bind(created_at)
    .bind(created_at)
    .fetch_one(pool)
    .await
}

pub async fn find_tag(pool: &PgPool, slug: &str) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM content_tags WHERE slug = $1 LIMIT 1")
        .bind(slug)
        .fetch_optional(pool)
        .await
}

pub async fn insert_tag(pool: &PgPool, name: &str, slug: &str) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar("INSERT INTO content_tags (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(name)
        .bind(slug)
        .fetch_one(pool)
        .await
}

pub async fn insert_entry_author(
    pool: &PgPool,
    entry_id: i32,
    author_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO content_entry_authors (entry_id, author_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(entry_id)
    .bind(author_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn insert_entry_tag(
    pool: &PgPool,
    entry_id: i32,
    tag_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO content_entry_tags (entry_id, tag_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(entry_id)
    .bind(tag_id)
    .execute(pool)
    .await?;

    Ok(())
}
