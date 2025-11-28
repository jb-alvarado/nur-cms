use sqlx::{Postgres, QueryBuilder, postgres::PgPool};
use strum::IntoEnumIterator;
use tracing::debug;

use crate::db::{
    fields::ContentEntryFields as CF,
    queries::{QueryObj, RespondObj, WhereBuilder},
    serialize::ContentEntrySerializer,
};
use crate::utils::errors::ServiceError;

#[cfg(debug_assertions)]
use crate::db::format_sql;
#[cfg(debug_assertions)]
use sqlx::Execute;

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

pub async fn select_content_entries(
    pool: &PgPool,
    query_obj: &QueryObj<CF>,
) -> Result<RespondObj<ContentEntrySerializer>, ServiceError> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut sep = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            CF::Authors => sep.push(format!("COALESCE(authors.data, '[]') AS {f}")),
            CF::Category => sep.push(format!("COALESCE(cats.data, '{{}}'::json) AS {f}")),
            CF::Tags => sep.push(format!("COALESCE(tags.data, ARRAY[]::record[]) AS {f}")),
            CF::Meta => sep.push(format!("(cm.data, cm.start_time, cm.end_time) AS {f}")),
            CF::Blocks => sep.push(format!("COALESCE(blocks.data, '[]') AS {f}")),
            CF::Body => sep.push("ce.text".to_string()),
            CF::GroupMembers => sep.push(format!("COALESCE(group_members.data, '[]') AS {f}")),
            CF::Embeds => sep.push(format!("COALESCE(embed_data.media, '[]') AS {f}")),
            CF::Media => sep.push("COALESCE(media.data, '{}'::json) AS \"media\""),
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
            Some(", '%'))))"),
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

    let query = query_builder.build_query_as::<ContentEntrySerializer>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()));

    let data: Vec<ContentEntrySerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(query_obj, data))
}
