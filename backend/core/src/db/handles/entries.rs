use serde::Serialize;
use serde_json::Value;
use sqlx::{Postgres, QueryBuilder, postgres::PgPool};
use strum::IntoEnumIterator;
use tracing::error;

#[cfg(debug_assertions)]
use sqlx::Execute;
#[cfg(debug_assertions)]
use tracing::debug;

use crate::db::{
    fields::{ContentEntryFields as CF, Table},
    handles::core::update_record,
    queries::{QueryObj, RespondObj, WhereBuilder},
    serialize::ContentEntrySerializer,
};
use crate::utils::errors::NurError;

#[cfg(debug_assertions)]
use crate::db::format_sql;

pub async fn delete_author_from_entry(
    pool: &PgPool,
    entry_id: i32,
    author_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM content_entry_tags WHERE entry_id = $1 AND author_id = $2")
        .bind(entry_id)
        .bind(author_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn delete_tag_from_entry(
    pool: &PgPool,
    entry_id: i32,
    tag_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM content_entry_tags WHERE entry_id = $1 AND tag_id = $2")
        .bind(entry_id)
        .bind(tag_id)
        .execute(pool)
        .await?;

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM content_entry_tags WHERE tag_id = $1")
            .bind(tag_id)
            .fetch_one(pool)
            .await?;

    if count == 0 {
        sqlx::query("DELETE FROM content_tags WHERE id = $1")
            .bind(tag_id)
            .execute(pool)
            .await?;
    }

    Ok(())
}

pub async fn upsert_entry_meta(
    pool: &PgPool,
    entry_id: i32,
    data: &Value,
) -> Result<(), sqlx::Error> {
    if let Some(meta_obj) = data.as_object() {
        let json_data = meta_obj.get("data").cloned();
        let start_time = meta_obj
            .get("start_time")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));
        let end_time = meta_obj
            .get("end_time")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));

        sqlx::query(
            r#"INSERT INTO content_meta (entry_id, data, start_time, end_time)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (entry_id) DO UPDATE SET
                   data = EXCLUDED.data,
                   start_time = EXCLUDED.start_time,
                   end_time = EXCLUDED.end_time"#,
        )
        .bind(entry_id)
        .bind(json_data)
        .bind(start_time)
        .bind(end_time)
        .execute(pool)
        .await?;
    }

    Ok(())
}

pub async fn select_media_id_by_path(
    pool: &PgPool,
    path: &str,
    filename: &str,
) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar::<_, i32>("SELECT id FROM media WHERE path = $1 AND filename = $2")
        .bind(path)
        .bind(filename)
        .fetch_optional(pool)
        .await
}

pub async fn delete_content_media_for_entry(
    pool: &PgPool,
    entry_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM content_media WHERE entry_id = $1")
        .bind(entry_id)
        .execute(pool)
        .await
        .map(|_| ())
}

pub async fn select_entry_text(
    pool: &PgPool,
    entry_id: i32,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>("SELECT text FROM content_entries WHERE id = $1")
        .bind(entry_id)
        .fetch_optional(pool)
        .await
}

pub async fn select_content_entries(
    pool: &PgPool,
    query_obj: &QueryObj<CF>,
) -> Result<RespondObj<ContentEntrySerializer>, NurError> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut sep = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            CF::Authors => sep.push(format!("COALESCE(authors.data, '[]') AS {f}")),
            CF::Category => sep.push(format!("COALESCE(cats.data, NULL) AS {f}")),
            CF::Tags => sep.push(format!("COALESCE(tags.data, ARRAY[]::record[]) AS {f}")),
            CF::Meta => sep.push(format!("(cm.data, cm.start_time, cm.end_time) AS {f}")),
            CF::Blocks => sep.push(format!("COALESCE(blocks.data, '[]') AS {f}")),
            CF::GroupMembers => sep.push(format!("COALESCE(group_members.data, '[]') AS {f}")),
            CF::Embeds => sep.push(format!("COALESCE(embed_data.media, '[]') AS {f}")),
            CF::Media => sep.push("COALESCE(media.data, NULL) AS \"media\""),
            CF::Text => match query_obj.character_limit {
                Some(limit) => sep.push(format!(
                    r#"regexp_replace(left(ce.{f}, {limit}), '\s+\S*$', ' …') as {f}"#
                )),
                None => sep.push(format!("ce.{f}")),
            },
            _ => sep.push(format!("ce.{f}")),
        };
    }

    sep.push("count(*) OVER() AS total_count");
    sep.push_unseparated(" ");
    query_builder.push("FROM content_entries ce ");

    if query_obj.type_slug.is_some() {
        query_builder.push("JOIN content_types ct ON ct.id = ce.type_id ");
    }

    if query_obj.fields.contains(&CF::Authors)
        || query_obj.author.is_some()
        || query_obj.search.is_some()
    {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', ca.id,
                        'first_name', ca.first_name,
                        'last_name', ca.last_name,
                        'slug', ca.slug
                    )
                    ORDER BY ca.last_name
                ) AS data
                FROM content_authors ca
                JOIN content_entry_authors cea ON cea.author_id = ca.id
                WHERE cea.entry_id = ce.id
            ) AS authors ON TRUE "#,
        );

        query_builder.push(
            r#"LEFT JOIN content_entry_authors cea ON cea.entry_id = ce.id
            LEFT JOIN content_authors ca ON ca.id = cea.author_id "#,
        );
    }

    if query_obj.fields.contains(&CF::Category) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT json_build_object(
                    'id', cc.id,
                    'group_id', cc.group_id,
                    'locale_id', cc.locale_id,
                    'name', cc.name,
                    'slug', cc.slug
                ) AS data
                FROM content_categories cc
                WHERE cc.id = ce.category_id
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

    if query_obj.fields.contains(&CF::Media) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT json_build_object(
                    'alt', m.alt,
                    'path', m.path,
                    'filename', m.filename,
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
                WHERE m.id = ce.media_id
            ) AS media ON TRUE "#,
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
                        'media_id', bl.media_id,
                        'order_index', bl.order_index,
                        'data', bl.data,
                        'media', CASE
                            WHEN bl.media_id IS NOT NULL THEN json_build_object(
                                'id', m.id,
                                'alt', m.alt,
                                'path', m.path,
                                'filename', m.filename,
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
                            ELSE NULL
                        END
                    )
                    ORDER BY bl.order_index
                ) AS data
                FROM content_blocks bl
                LEFT JOIN media m ON m.id = bl.media_id
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

    if query_obj.fields.contains(&CF::Embeds) {
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
            ) AS embed_data ON TRUE "#,
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
            "EXISTS (
                SELECT 1
                FROM content_entry_authors cea2
                JOIN content_authors ca2 ON ca2.id = cea2.author_id
                WHERE cea2.entry_id = ce.id
                AND (ca2.first_name ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%') "),
        );

        where_chain.push_and_bind(
            Some("OR"),
            "ca2.last_name ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%'))"),
        );

        where_chain.push_and_bind(
            Some("OR"),
            "ce.text_vector @@ websearch_to_tsquery((SELECT tsv_dict::regconfig FROM locales WHERE id = ce.locale_id), ",
            search,
            Some(")))"),
        );
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

    let query = query_builder.build_query_as::<ContentEntrySerializer>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()));

    let data: Vec<ContentEntrySerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(query_obj, data))
}

/// Synchronizes blocks array with the content_blocks table.
///
/// This function reconciles the blocks array from the update request with the database.
/// It handles:
/// - Inserting new blocks
/// - Updating existing blocks
/// - Deleting blocks that are no longer in the array
pub async fn sync_entry_blocks(
    pool: &PgPool,
    entry_id: i32,
    blocks: &[Value],
) -> Result<(), NurError> {
    // Get existing blocks from database
    let existing_blocks: Vec<(i32, i32, Value)> = sqlx::query_as(
        "SELECT id, order_index, data FROM content_blocks WHERE entry_id = $1 ORDER BY order_index",
    )
    .bind(entry_id)
    .fetch_all(pool)
    .await?;

    let mut existing_ids = Vec::new();
    let mut tx = pool.begin().await?;

    // Process each block in the incoming array
    for (index, block_value) in blocks.iter().enumerate() {
        let block_obj = match block_value.as_object() {
            Some(obj) => obj,
            None => {
                error!("Block at index {index} is not a valid object");
                continue;
            }
        };

        let block_id = block_obj
            .get("id")
            .and_then(Value::as_i64)
            .map(|v| v as i32);
        let media_id = block_obj
            .get("media_id")
            .and_then(Value::as_i64)
            .map(|v| v as i32);
        let data = block_obj.get("data").cloned().unwrap_or(Value::Null);
        let order_index = block_obj
            .get("order_index")
            .and_then(Value::as_i64)
            .map(|v| v as i32)
            .unwrap_or(index as i32);

        match block_id {
            Some(id) => {
                // Update existing block
                existing_ids.push(id);
                sqlx::query(
                    "UPDATE content_blocks SET media_id = $1, order_index = $2, data = $3 WHERE id = $4 AND entry_id = $5"
                )
                .bind(media_id)
                .bind(order_index)
                .bind(&data)
                .bind(id)
                .bind(entry_id)
                .execute(&mut *tx)
                .await?;
            }
            None => {
                // Insert new block
                sqlx::query(
                    "INSERT INTO content_blocks (entry_id, media_id, order_index, data) VALUES ($1, $2, $3, $4)",
                )
                .bind(entry_id)
                .bind(media_id)
                .bind(order_index)
                .bind(&data)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    // Delete blocks that are no longer in the array
    for (db_id, _, _) in &existing_blocks {
        if !existing_ids.contains(db_id) {
            sqlx::query("DELETE FROM content_blocks WHERE id = $1")
                .bind(db_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

/// Updates a content entry and synchronizes its blocks.
///
/// This function combines the standard update_record logic with special handling for blocks.
pub async fn update_entry_with_blocks<T>(
    pool: &PgPool,
    entry_id: i32,
    data: &T,
) -> Result<(), NurError>
where
    T: Serialize,
{
    let mut value = serde_json::to_value(data)?;

    let obj = match value.as_object() {
        Some(map) => map.clone(),
        None => return Ok(()),
    };

    // Extract blocks if present
    let blocks = if let Some(Value::Array(arr)) = obj.get("blocks") {
        Some(arr.clone())
    } else {
        None
    };

    if let Some(meta) = value.get("meta") {
        upsert_entry_meta(pool, entry_id, meta).await?;

        if let Some(obj) = value.as_object_mut() {
            obj.remove("meta");
        }
    }

    // Update the entry record (blocks will be ignored by update_record)
    update_record(pool, &Table::ContentEntries, entry_id, &value).await?;

    // Sync blocks if they were provided
    if let Some(blocks_data) = blocks {
        sync_entry_blocks(pool, entry_id, &blocks_data).await?;
    }

    Ok(())
}
