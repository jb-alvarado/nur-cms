use markdown::{ParseOptions, to_mdast};
use serde_json::Value;
use sqlx::{PgConnection, Postgres, QueryBuilder, postgres::PgPool};
use strum::IntoEnumIterator;
use tracing::error;

#[cfg(debug_assertions)]
use sqlx::Execute;
#[cfg(debug_assertions)]
use tracing::debug;

use crate::db::{
    fields::{ContentEntryFields as CF, ContentNodeFields as CN, Table},
    handles::core::update_record,
    queries::{QueryObj, RespondObj, WhereBuilder},
    serialize::ContentEntrySerializer,
};
use crate::utils::{ast_serialize::persist_content_media, errors::NurError};

#[cfg(debug_assertions)]
use crate::db::format_sql;

const AUTHOR_JOIN: &str = r#"LEFT JOIN LATERAL (
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
    ) AS authors ON TRUE
    LEFT JOIN content_entry_authors cea ON cea.entry_id = ce.id
    LEFT JOIN content_authors ca ON ca.id = cea.author_id "#;

const CATEGORY_JOIN: &str = r#"LEFT JOIN LATERAL (
        SELECT json_build_object(
            'id', cc.id,
            'group_id', cc.group_id,
            'locale_id', cc.locale_id,
            'name', cc.name,
            'slug', cc.slug
        ) AS data
        FROM content_categories cc
        WHERE cc.id = ce.category_id
    ) AS cats ON TRUE "#;

const TAG_JOIN: &str = r#"LEFT JOIN LATERAL (
        SELECT ARRAY_AGG(
            (t.id, t.name, t.slug)
        ) AS data
        FROM content_tags t
        JOIN content_entry_tags cet ON cet.tag_id = t.id
        WHERE cet.entry_id = ce.id
    ) AS tags ON TRUE "#;

const MEDIA_JOIN: &str = r#"LEFT JOIN LATERAL (
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
    ) AS media ON TRUE "#;

const GROUP_JOIN: &str = r#"LEFT JOIN LATERAL (
        SELECT jsonb_agg(
            jsonb_build_object(
                'id', ge.id,
                'locale_id', ge.locale_id
            )
        ) AS data
        FROM content_entries ge
        WHERE ge.group_id = ce.group_id
            AND ge.id != ce.id
    ) AS group_members ON TRUE "#;

fn search_content(where_chain: &mut WhereBuilder<'_>, search: String) {
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

fn nodes_join(query_obj: &QueryObj<CF>) -> String {
    let text = match query_obj.character_limit {
        Some(limit) => format!(r#"regexp_replace(left(cn.text, {limit}), '\s+\S*$', ' …')"#),
        None => "cn.text".to_string(),
    };

    let mut fields = Vec::new();
    let needs_embeds = query_obj
        .fields
        .iter()
        .any(|f| matches!(f, CF::Node(CN::Text) | CF::Node(CN::Embeds)));

    for f in &query_obj.fields {
        match *f {
            CF::Node(CN::ID) => fields.push("'id', cn.id".to_string()),
            CF::Node(CN::OrderIndex) => fields.push("'order_index', cn.order_index".to_string()),
            CF::Node(CN::Text) => fields.push(format!("'text', {text}")),
            CF::Node(CN::Data) => fields.push("'data', cn.data".to_string()),
            CF::Node(CN::Embeds) => {
                fields.push("'embeds', COALESCE(embed_data.media, '[]')".to_string());
            }
            CF::Node(CN::Media) => fields.push(
                r#"'media', CASE
                    WHEN cn.media_id IS NOT NULL THEN json_build_object(
                        'id', m.id,
                        'alt', m.alt,
                        'path', m.path,
                        'filename', m.filename,
                        'variants', COALESCE(
                            (SELECT json_agg(json_build_object(
                                'id', mv.id,
                                'width', mv.width,
                                'height', mv.height,
                                'filename', mv.filename
                            )) FROM media_variants mv WHERE mv.media_id = m.id),
                            '[]'
                        )
                    )
                    ELSE NULL
                END"#
                    .to_string(),
            ),
            CF::Node(CN::ParentID) => fields.push("'parent_id', cn.parent_id".to_string()),
            _ => (),
        }
    }

    let mut from_clause = "FROM content_nodes cn".to_string();

    if query_obj.fields.contains(&CF::Node(CN::Media)) {
        from_clause.push_str(" LEFT JOIN media m ON m.id = cn.media_id");
    }

    if needs_embeds {
        from_clause.push_str(
            r#"
        LEFT JOIN LATERAL (
            SELECT json_agg(
                json_build_object(
                    'id', m.id,
                    'alt', m.alt,
                    'filename', m.filename,
                    'path', m.path,
                    'type', m.type,
                    'ast_line', cnm.ast_line,
                    'start_offset', cnm.start_offset,
                    'end_offset', cnm.end_offset,
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
            FROM content_node_media cnm
            JOIN media m ON m.id = cnm.media_id
            WHERE cnm.node_id = cn.id
        ) AS embed_data ON TRUE"#,
        );
    }

    format!(
        r#"LEFT JOIN LATERAL (
        SELECT jsonb_agg(
            jsonb_build_object(
                {}
            ) ORDER BY cn.order_index
        ) AS nodes
        {}
        WHERE cn.entry_id = ce.id
    ) AS nodes ON TRUE "#,
        fields.join(", "),
        from_clause
    )
}

pub async fn select_content_entries(
    pool: &PgPool,
    query_obj: &QueryObj<CF>,
) -> Result<RespondObj<ContentEntrySerializer>, NurError> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut sep = qb.separated(", ");
    let mut add_node = true;

    for f in &query_obj.fields {
        match *f {
            CF::Authors => sep.push(format!("COALESCE(authors.data, '[]') AS {f}")),
            CF::Category => sep.push(format!("COALESCE(cats.data, NULL) AS {f}")),
            CF::Tags => sep.push(format!("COALESCE(tags.data, ARRAY[]::record[]) AS {f}")),
            CF::Meta => sep.push(format!("(cm.start_time, cm.end_time) AS {f}")),
            CF::Node(_) => {
                if add_node {
                    add_node = false;
                    sep.push("COALESCE(nodes, '[]') AS nodes".to_string())
                } else {
                    continue;
                }
            }
            CF::GroupMembers => sep.push(format!("COALESCE(group_members.data, '[]') AS {f}")),
            CF::Media => sep.push("COALESCE(media.data, NULL) AS \"media\""),
            _ => sep.push(format!("ce.{f}")),
        };
    }

    sep.push("count(*) OVER() AS total_count");
    sep.push_unseparated(" ");
    qb.push("FROM content_entries ce ");

    if query_obj.type_slug.is_some() {
        qb.push("JOIN content_types ct ON ct.id = ce.type_id ");
    }

    if query_obj.fields.contains(&CF::Authors)
        || query_obj.author.is_some()
        || query_obj.search.is_some()
    {
        qb.push(AUTHOR_JOIN);
    }

    if query_obj.fields.contains(&CF::Category) {
        qb.push(CATEGORY_JOIN);
    }

    if query_obj.fields.contains(&CF::Tags) {
        qb.push(TAG_JOIN);
    }

    if query_obj.fields.contains(&CF::Meta)
        || query_obj.start_time.is_some()
        || query_obj.end_time.is_some()
    {
        qb.push("LEFT JOIN content_meta cm ON cm.entry_id = ce.id ");
    }

    if query_obj.fields.contains(&CF::Media) {
        qb.push(MEDIA_JOIN);
    }

    if query_obj.fields.iter().any(|f| matches!(f, CF::Node(_))) {
        qb.push(nodes_join(query_obj));
    }

    if query_obj.fields.contains(&CF::GroupMembers) {
        qb.push(GROUP_JOIN);
    }

    let mut where_chain = WhereBuilder::new(qb);

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
        search_content(&mut where_chain, search);
    }

    // take builder back from where_chain
    qb = where_chain.into_inner();

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
        qb.push(format!(" ORDER BY {}", ordering));
    }

    qb.push(format!(
        " LIMIT {} OFFSET {}",
        query_obj.limit, query_obj.offset
    ));

    let query = qb.build_query_as::<ContentEntrySerializer>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()));

    let data: Vec<ContentEntrySerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(query_obj, data))
}

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
    pool: &mut PgConnection,
    node_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM content_node_media WHERE node_id = $1")
        .bind(node_id)
        .execute(pool)
        .await
        .map(|_| ())
}

pub async fn select_entry_text(pool: &PgPool, node_id: i32) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>("SELECT text FROM content_entries WHERE id = $1")
        .bind(node_id)
        .fetch_optional(pool)
        .await
}

/// Synchronizes nodes array with the content_nodes table.
///
/// This function reconciles the nodes array from the update request with the database.
/// It handles:
/// - Simple nodes (text or data)
/// - Nodes with blocks (parent + children)
/// - Inserting new nodes
/// - Updating existing nodes
/// - Deleting nodes that are no longer in the array
pub async fn sync_entry_nodes(
    pool: &PgPool,
    entry_id: i32,
    nodes: &[Value],
) -> Result<(), NurError> {
    // Get existing nodes from database
    // TODO: i32, Option<String>, Option<Value> are not needed
    let existing_nodes: Vec<(i64, i32, Option<String>, Option<Value>)> = sqlx::query_as(
        "SELECT id, order_index, text, data FROM content_nodes WHERE entry_id = $1 ORDER BY order_index",
    )
    .bind(entry_id)
    .fetch_all(pool)
    .await?;

    let mut existing_ids = Vec::new();
    let mut tx = pool.begin().await?;
    let mut order_index = 1;

    // Process each node in the incoming array
    for node in nodes {
        let node_obj = match node.as_object() {
            Some(obj) => obj,
            None => {
                error!("Node is not a valid object");
                continue;
            }
        };

        // Check if this node has blocks (nested structure)
        if let Some(blocks_arr) = node_obj.get("blocks").and_then(|b| b.as_array()) {
            let mut parent_id: Option<i64> = None;

            // Process each block in the blocks array
            for block_value in blocks_arr {
                let block_obj = match block_value.as_object() {
                    Some(obj) => obj,
                    None => {
                        error!("Block in node is not a valid object");
                        continue;
                    }
                };

                let node_id = block_obj.get("id").and_then(Value::as_i64);
                let media_id = block_obj
                    .get("media_id")
                    .and_then(Value::as_i64)
                    .map(|v| v as i32);
                let data = block_obj.get("data").cloned().unwrap_or(Value::Null);

                match node_id {
                    Some(id) => {
                        // Update existing node
                        existing_ids.push(id);
                        sqlx::query(
                            "UPDATE content_nodes SET media_id = $1, order_index = $2, data = $3, parent_id = $4 WHERE id = $5 AND entry_id = $6"
                        )
                        .bind(media_id)
                        .bind(order_index)
                        .bind(&data)
                        .bind(parent_id)
                        .bind(id)
                        .bind(entry_id)
                        .execute(&mut *tx)
                        .await?;
                    }
                    None => {
                        // Insert new node
                        let new_node_id: i64 = sqlx::query_scalar(
                            "INSERT INTO content_nodes (entry_id, media_id, order_index, data, parent_id) VALUES ($1, $2, $3, $4, $5) RETURNING id",
                        )
                        .bind(entry_id)
                        .bind(media_id)
                        .bind(order_index)
                        .bind(&data)
                        .bind(parent_id)
                        .fetch_one(&mut *tx)
                        .await?;

                        if parent_id.is_none() {
                            parent_id = Some(new_node_id);
                        }

                        existing_ids.push(new_node_id);
                    }
                }

                order_index += 1;
            }
        } else {
            // Simple node (not a block container)
            let node_id = node_obj.get("id").and_then(Value::as_i64);
            let media_id = node_obj
                .get("media_id")
                .and_then(Value::as_i64)
                .map(|v| v as i32);
            let text = node_obj.get("text").and_then(|t| t.as_str());
            let data = node_obj.get("data").cloned().unwrap_or(Value::Null);

            match node_id {
                Some(id) => {
                    // Update existing node
                    existing_ids.push(id);
                    sqlx::query(
                        "UPDATE content_nodes SET media_id = $1, order_index = $2, text = $3, data = $4 WHERE id = $5 AND entry_id = $6"
                    )
                    .bind(media_id)
                    .bind(order_index)
                    .bind(text)
                    .bind(&data)
                    .bind(id)
                    .bind(entry_id)
                    .execute(&mut *tx)
                    .await?;

                    // Process text and media
                    if let Some(text_str) = text
                        && !text_str.is_empty()
                    {
                        let ast = to_mdast(text_str, &ParseOptions::default())?;
                        let tree: Value = serde_json::to_value(ast).unwrap_or_default();
                        delete_content_media_for_entry(&mut tx, entry_id).await?;
                        persist_content_media(pool, id, &tree).await?;
                    }
                }
                None => {
                    // Insert new node
                    let new_node_id: i64 = sqlx::query_scalar(
                        "INSERT INTO content_nodes (entry_id, media_id, order_index, text, data) VALUES ($1, $2, $3, $4, $5) RETURNING id",
                    )
                    .bind(entry_id)
                    .bind(media_id)
                    .bind(order_index)
                    .bind(text)
                    .bind(&data)
                    .fetch_one(&mut *tx)
                    .await?;

                    // Process text and media
                    if let Some(text_str) = text
                        && !text_str.is_empty()
                    {
                        let ast = to_mdast(text_str, &ParseOptions::default())?;
                        let tree: Value = serde_json::to_value(ast).unwrap_or_default();
                        persist_content_media(pool, new_node_id, &tree).await?;
                    }

                    existing_ids.push(new_node_id);
                }
            }

            order_index += 1;
        }
    }

    // Delete nodes that are no longer in the array
    for (db_id, _, _, _) in &existing_nodes {
        if !existing_ids.contains(db_id) {
            sqlx::query("DELETE FROM content_nodes WHERE id = $1")
                .bind(db_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

/// Updates a content entry and synchronizes its nodes.
///
/// This function combines the standard update_record logic with special handling for nodes.
pub async fn update_entry_with_nodes(
    pool: &PgPool,
    entry_id: i32,
    content: &Value,
) -> Result<(), NurError> {
    if let Some(meta) = content.get("meta") {
        upsert_entry_meta(pool, entry_id, meta).await?;
    }

    // Update the entry record (nodes will be ignored by update_record)
    update_record(pool, &Table::ContentEntries, entry_id, &content).await?;

    if let Some(nodes) = content.get("nodes").as_ref().and_then(|b| b.as_array()) {
        sync_entry_nodes(pool, entry_id, nodes).await?;
    }

    Ok(())
}
