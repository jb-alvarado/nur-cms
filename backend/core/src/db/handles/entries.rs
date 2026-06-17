use markdown::{ParseOptions, to_mdast};
use serde_json::Value;
use sqlx::{Postgres, QueryBuilder, postgres::PgPool};
use strum::IntoEnumIterator;
use tracing::error;

#[cfg(debug_assertions)]
use colored::Colorize;
#[cfg(debug_assertions)]
use tracing::debug;

use crate::db::{
    fields::{
        ContentAuthorFields, ContentCategoryFields, ContentEntryFields as CF,
        ContentNodeFields as CN, Table,
    },
    handles::core::update_record,
    queries::{QueryObj, RespondObj, WhereBuilder},
    serialize::ContentEntrySerializer,
};
use crate::utils::{ast_serialize::persist_content_media, errors::NurError};

#[cfg(debug_assertions)]
use crate::db::format_sql;

type ContentNodeRecord = (i64, i32, Option<String>, Option<String>, Option<Value>);

fn tag_join(entry_alias: &str) -> String {
    format!(
        r#"LEFT JOIN LATERAL (
            SELECT COALESCE(
                array_agg(ROW(t.id, t.name, t.slug) ORDER BY t.name),
                ARRAY[]::record[]
            ) AS data
            FROM content_tags t
            JOIN content_entry_tags cet ON cet.tag_id = t.id
            WHERE cet.entry_id = {e}.id
        ) AS tags ON TRUE "#,
        e = entry_alias
    )
}

fn variants_lateral(media_alias: &str, variants_alias: &str) -> String {
    format!(
        r#"LEFT JOIN LATERAL (
            SELECT COALESCE(
                json_agg(
                    json_build_object(
                        'id', mv.id,
                        'width', mv.width,
                        'height', mv.height,
                        'filename', mv.filename
                    )
                    ORDER BY mv.id
                ),
                '[]'::json
            ) AS variants
            FROM media_variants mv
            WHERE mv.media_id = {media_alias}.id
        ) AS {variants_alias} ON TRUE "#,
        media_alias = media_alias,
        variants_alias = variants_alias
    )
}

fn media_join(entry_alias: &str) -> String {
    let mut s = String::new();

    s.push_str(
        r#"LEFT JOIN LATERAL (
            SELECT json_build_object(
                'alt', m.alt,
                'path', m.path,
                'filename', m.filename,
                'variants', mv.variants
            ) AS data
            FROM media m
        "#,
    );

    s.push_str(&variants_lateral("m", "mv"));

    s.push_str(&format!(
        r#"WHERE m.id = {entry}.media_id
        ) AS media ON TRUE "#,
        entry = entry_alias
    ));

    s
}

fn group_join(entry_alias: &str) -> String {
    format!(
        r#"LEFT JOIN LATERAL (
            SELECT COALESCE(
                jsonb_agg(
                    jsonb_build_object(
                        'id', ge.id,
                        'locale_code', l.code,
                        'locale_name', l.name
                    )
                ),
                '[]'::jsonb
            ) AS data
            FROM content_entries ge
            JOIN locales l ON l.id = ge.locale_id
            WHERE ge.group_id = {e}.group_id
        ) AS group_members ON TRUE "#,
        e = entry_alias
    )
}

fn authors_join(query_obj: &QueryObj<CF>, entry_alias: &str, include_filter_joins: bool) -> String {
    let mut fields = Vec::new();
    let needs_media = query_obj
        .fields
        .iter()
        .any(|f| matches!(f, CF::Author(ContentAuthorFields::Media)));

    for f in &query_obj.fields {
        match f {
            CF::Author(ContentAuthorFields::Media) => {
                fields.push(format!(
                    r#"'media', CASE
                    WHEN ca2.media_id IS NOT NULL THEN (
                        SELECT json_build_object(
                            'id', m.id,
                            'alt', m.alt,
                            'path', m.path,
                            'filename', m.filename,
                            'variants', mv.variants
                        )
                        FROM media m
                        {}
                        WHERE m.id = ca2.media_id
                    )
                    ELSE NULL
                END"#,
                    variants_lateral("m", "mv")
                ));
            }
            CF::Author(author_field) => {
                let field = format!("'{}', ca2.{}", author_field, author_field);

                if !fields.contains(&field) {
                    fields.push(field);
                }
            }
            _ => (),
        }
    }

    let mut join = String::new();

    if !fields.is_empty() {
        let media_id_select = if needs_media {
            "ca2.media_id AS media_id,"
        } else {
            ""
        };

        join.push_str(&format!(
            r#"LEFT JOIN LATERAL (
        SELECT jsonb_agg(
            jsonb_build_object(
                {}
            )
            ORDER BY ca2.last_name
        ) AS data
        FROM (
            SELECT DISTINCT
                ca2.id,
                ca2.first_name,
                ca2.last_name,
                ca2.slug,
                ca2.bio,
                {}
                ca2.created_at,
                ca2.updated_at
            FROM content_authors ca2
            JOIN content_entry_authors cea2 ON cea2.author_id = ca2.id
            WHERE cea2.entry_id = {}.id
        ) ca2
    ) AS authors ON TRUE "#,
            fields.join(", "),
            media_id_select,
            entry_alias
        ));
    }

    if include_filter_joins {
        join.push_str(&format!(
            "LEFT JOIN content_entry_authors cea ON cea.entry_id = {}.id LEFT JOIN content_authors ca ON ca.id = cea.author_id ",
            entry_alias
        ));
    }

    join
}

fn category_join(query_obj: &QueryObj<CF>, entry_alias: &str) -> String {
    let mut fields = Vec::new();

    for f in &query_obj.fields {
        match f {
            CF::Category(ContentCategoryFields::Media) => {
                fields.push(format!(
                    r#"'media', CASE
                            WHEN cc2.media_id IS NOT NULL THEN (
                                SELECT json_build_object(
                                    'id', m.id,
                                    'alt', m.alt,
                                    'path', m.path,
                                    'filename', m.filename,
                                    'variants', mv.variants
                                )
                                FROM media m
                                {}
                                WHERE m.id = cc2.media_id
                            )
                            ELSE NULL
                        END"#,
                    variants_lateral("m", "mv")
                ));
            }
            CF::Category(ContentCategoryFields::GroupMembers) => {
                fields.push(
                    r#"'group_members', COALESCE(
                        (
                            SELECT jsonb_agg(
                                jsonb_build_object(
                                    'id', cc3.id,
                                    'locale_id', cc3.locale_id
                                )
                            )
                            FROM content_categories cc3
                            WHERE cc3.group_id = cc2.group_id
                        ),
                        '[]'
                    )"#
                    .to_string(),
                );
            }
            CF::Category(category_field) => {
                let field = format!("'{}', cc2.{}", category_field, category_field);

                if !fields.contains(&field) {
                    fields.push(field);
                }
            }
            _ => (),
        }
    }

    if fields.is_empty() {
        return String::new();
    }

    format!(
        r#"LEFT JOIN LATERAL (
        SELECT json_build_object(
            {}
        ) AS data
        FROM content_categories cc2
        WHERE cc2.id = {}.category_id
    ) AS cats ON TRUE "#,
        fields.join(", "),
        entry_alias
    )
}

fn nodes_join(query_obj: &QueryObj<CF>, entry_alias: &str) -> String {
    let mut fields = Vec::new();
    let mut null_check_fields = Vec::new();

    let needs_embeds = query_obj
        .fields
        .iter()
        .any(|f| matches!(f, CF::Node(CN::Text) | CF::Node(CN::Embeds)));

    for f in &query_obj.fields {
        match *f {
            CF::Node(CN::ID) => fields.push("'id', cn.id".to_string()),
            CF::Node(CN::OrderIndex) => fields.push("'order_index', cn.order_index".to_string()),
            CF::Node(CN::Name) => {
                fields.push("'name', cn.name".to_string());
                null_check_fields.push("cn.name".to_string());
            }
            CF::Node(CN::Text) => {
                fields.push("'text', cn.text".to_string());
                null_check_fields.push("cn.text".to_string());
            }
            CF::Node(CN::Data) => {
                fields.push("'data', cn.data".to_string());
                null_check_fields.push("cn.data".to_string());
            }
            CF::Node(CN::Embeds) => {
                fields.push("'embeds', COALESCE(embed_data.media, '[]'::json)".to_string());
            }
            CF::Node(CN::Media) => {
                fields.push(format!(
                    r#"'media', CASE
                            WHEN cn.media_id IS NOT NULL THEN (
                                SELECT json_build_object(
                                    'id', m.id,
                                    'alt', m.alt,
                                    'path', m.path,
                                    'filename', m.filename,
                                    'variants', mv.variants
                                )
                                FROM media m
                                {}
                                WHERE m.id = cn.media_id
                            )
                            ELSE NULL
                        END"#,
                    variants_lateral("m", "mv")
                ));
                null_check_fields.push("cn.media_id".to_string());
            }
            CF::Node(CN::ParentID) => {
                fields.push("'parent_id', cn.parent_id".to_string());
                null_check_fields.push("cn.parent_id".to_string());
            }
            CF::Node(CN::Blocks) => {
                if !query_obj.fields.contains(&CF::Node(CN::ID)) {
                    fields.push("'id', cn.id".to_string());
                }
                if !query_obj.fields.contains(&CF::Node(CN::OrderIndex)) {
                    fields.push("'order_index', cn.order_index".to_string());
                }
                if !query_obj.fields.contains(&CF::Node(CN::Data)) {
                    fields.push("'data', cn.data".to_string());
                }
                if !query_obj.fields.contains(&CF::Node(CN::ParentID)) {
                    fields.push("'parent_id', cn.parent_id".to_string());
                }
            }
            _ => (),
        }
    }

    let mut from_clause = "FROM content_nodes cn".to_string();

    if needs_embeds {
        from_clause.push_str(
            r#"
            LEFT JOIN LATERAL (
                SELECT COALESCE(
                    json_agg(
                        json_build_object(
                            'id', m.id,
                            'alt', m.alt,
                            'filename', m.filename,
                            'path', m.path,
                            'type', m.type,
                            'ast_line', cnm.ast_line,
                            'variants', mv.variants
                        )
                        ORDER BY cnm.ast_line, cnm.start_offset, cnm.end_offset
                    ),
                    '[]'::json
                ) AS media
                FROM content_node_media cnm
                JOIN media m ON m.id = cnm.media_id
        "#,
        );

        from_clause.push_str(&variants_lateral("m", "mv"));

        from_clause.push_str(
            r#" WHERE cnm.node_id = cn.id
            ) AS embed_data ON TRUE"#,
        );
    }

    let sort = if query_obj.blocks_random {
        "random()"
    } else {
        "cn.order_index"
    };

    let limit = match query_obj.blocks_limit {
        Some(limit) => format!("LIMIT {limit}"),
        None => String::new(),
    };

    let null_check = if null_check_fields.is_empty() {
        String::new()
    } else {
        format!(
            " AND ({})",
            null_check_fields
                .iter()
                .map(|f| format!("{f} IS NOT NULL"))
                .collect::<Vec<_>>()
                .join(" OR ")
        )
    };

    format!(
        r#"LEFT JOIN LATERAL (
            SELECT jsonb_agg(node_json ORDER BY sort_key) AS nodes
            FROM (
                SELECT {sort} AS sort_key,
                       jsonb_build_object({fields}) AS node_json
                {from_clause}
                WHERE cn.entry_id = {entry}.id{null_check}
                {limit}
            ) AS node_data
        ) AS nodes ON TRUE "#,
        sort = sort,
        fields = fields.join(", "),
        from_clause = from_clause,
        entry = entry_alias,
        null_check = null_check,
        limit = limit
    )
}

fn comment_count_join(entry_alias: &str) -> String {
    format!(
        r#"LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS comment_count
            FROM comments c
            WHERE c.entry_id = {e}.id
              AND c.status = 'approved'
        ) AS cc ON TRUE "#,
        e = entry_alias
    )
}

pub async fn select_content_entries(
    pool: &PgPool,
    query_obj: &QueryObj<CF>,
) -> Result<RespondObj<ContentEntrySerializer>, NurError> {
    let ordering_with_alias = |entry_alias: &str| {
        query_obj
            .ordering
            .split(',')
            .filter_map(|part| {
                let mut split = part.split_whitespace();
                let field = split.next()?.trim();
                let direction = split.next().unwrap_or("ASC").to_uppercase();

                if direction != "ASC" && direction != "DESC" {
                    return None;
                }

                let mapped = match field {
                    "author" | "author.last_name" => Some(format!(
                        "(SELECT MIN(ca.last_name) FROM content_entry_authors cea JOIN content_authors ca ON ca.id = cea.author_id WHERE cea.entry_id = {}.id)",
                        entry_alias
                    )),
                    "author.id" => Some(format!(
                        "(SELECT MIN(ca.id) FROM content_entry_authors cea JOIN content_authors ca ON ca.id = cea.author_id WHERE cea.entry_id = {}.id)",
                        entry_alias
                    )),
                    "author.first_name" => Some(format!(
                        "(SELECT MIN(ca.first_name) FROM content_entry_authors cea JOIN content_authors ca ON ca.id = cea.author_id WHERE cea.entry_id = {}.id)",
                        entry_alias
                    )),
                    "author.slug" => Some(format!(
                        "(SELECT MIN(ca.slug) FROM content_entry_authors cea JOIN content_authors ca ON ca.id = cea.author_id WHERE cea.entry_id = {}.id)",
                        entry_alias
                    )),
                    "author.created_at" => Some(format!(
                        "(SELECT MIN(ca.created_at) FROM content_entry_authors cea JOIN content_authors ca ON ca.id = cea.author_id WHERE cea.entry_id = {}.id)",
                        entry_alias
                    )),
                    "author.updated_at" => Some(format!(
                        "(SELECT MIN(ca.updated_at) FROM content_entry_authors cea JOIN content_authors ca ON ca.id = cea.author_id WHERE cea.entry_id = {}.id)",
                        entry_alias
                    )),
                    "start_time" => Some(format!(
                        "(SELECT cm.start_time FROM content_meta cm WHERE cm.entry_id = {}.id)",
                        entry_alias
                    )),
                    "end_time" => Some(format!(
                        "(SELECT cm.end_time FROM content_meta cm WHERE cm.entry_id = {}.id)",
                        entry_alias
                    )),
                    _ if CF::iter().any(|f| f.to_string() == field) => {
                        Some(format!("{entry_alias}.{field}"))
                    }
                    _ => None,
                }?;

                Some(format!("{mapped} {direction}"))
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    let page_ordering = ordering_with_alias("f");
    let picked_ordering = ordering_with_alias("picked");
    let outer_ordering = ordering_with_alias("p");

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new("");

    let lang = match query_obj.grouped {
        true => ", l.code AS locale_slug",
        false => "",
    };

    if let Some(search) = query_obj.search.clone() {
        qb.push("WITH search_input AS ( SELECT trim(");
        qb.push_bind(search);
        qb.push(format!(") AS q ), base_entries AS ( SELECT ce.id, ce.title, ce.slug, ce.status, ce.created_at, ce.updated_at, ce.locale_id, ce.group_id, ce.type_id, ce.category_id, ce.media_id{lang} FROM content_entries ce "));
    } else {
        qb.push(format!(
            "WITH filtered AS NOT MATERIALIZED ( SELECT ce.*{lang} FROM content_entries ce "
        ));
    }

    if query_obj.type_slug.is_some() {
        qb.push("JOIN content_types ct ON ct.id = ce.type_id ");
    }

    if query_obj.grouped {
        qb.push("JOIN locales l ON l.id = ce.locale_id ");
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

    if let Some(excluded_types) = &query_obj.exclude_types {
        where_chain.push_and_bind(
            None,
            "NOT (ce.type_id = ANY(",
            excluded_types.clone(),
            Some("::int4[]))"),
        );
    }

    if let Some(slug) = &query_obj.search_slug {
        where_chain.push_and_bind(None, "ce.slug = ", slug, None);
    }

    if let Some(category_slug) = &query_obj.category_slug {
        where_chain.push_and_bind(
            None,
            "EXISTS (SELECT 1 FROM content_categories cc WHERE cc.id = ce.category_id AND cc.slug = ",
            category_slug,
            Some(")"),
        );
    }

    if let Some(locale_code) = &query_obj.locale_code
        && !query_obj.grouped
    {
        where_chain.push_and_bind(
            None,
            "EXISTS (SELECT 1 FROM locales l WHERE l.id = ce.locale_id AND l.code = ",
            locale_code,
            Some(")"),
        );
    }

    if let Some(status) = &query_obj.search_status {
        where_chain.push_and_bind(None, "ce.status = ", status, None);
    }

    if let Some(slug) = &query_obj.author_slug {
        where_chain.push_and_bind(
            None,
            r#"EXISTS (
                SELECT 1
                FROM content_entry_authors cea
                JOIN content_authors ca ON ca.id = cea.author_id
                WHERE cea.entry_id = ce.id
                  AND ca.slug = "#,
            slug,
            Some(")"),
        );
    }

    if let Some(slug) = &query_obj.tag_slug {
        where_chain.push_and_bind(
            None,
            r#"EXISTS (
                SELECT 1
                FROM content_entry_tags cet
                JOIN content_tags t ON t.id = cet.tag_id
                WHERE cet.entry_id = ce.id
                  AND t.slug = "#,
            slug.clone(),
            Some(")"),
        );
    }

    if let Some(id) = &query_obj.group_id {
        where_chain.push_and_bind(None, "ce.group_id = ", id, None);
    }

    if let Some(start) = &query_obj.start_time {
        where_chain.push_and_bind(
            None,
            "EXISTS (SELECT 1 FROM content_meta cm WHERE cm.entry_id = ce.id AND cm.start_time >= ",
            start,
            Some(")"),
        );
    }

    if let Some(end) = &query_obj.end_time {
        where_chain.push_and_bind(
            None,
            "EXISTS (SELECT 1 FROM content_meta cm WHERE cm.entry_id = ce.id AND cm.end_time <= ",
            end,
            Some(")"),
        );
    }

    qb = where_chain.into_inner();

    if query_obj.search.is_some() {
        qb.push(
            r#" ),
            locale_queries AS (
                SELECT
                    l.id AS locale_id,
                    phraseto_tsquery(l.tsv_dict::regconfig, si.q) AS phrase_q,
                    make_phrase_prefix_tsquery(l.tsv_dict::regconfig, si.q) AS phrase_prefix_q,
                    make_last_term_prefix_tsquery(l.tsv_dict::regconfig, si.q) AS prefix_q
                FROM locales l
                CROSS JOIN search_input si
            ),
            title_hits AS (
                SELECT
                    be.id,
                    CASE
                        WHEN lower(trim(be.title)) = lower(si.q) THEN 500
                        WHEN lower(trim(be.title)) LIKE lower(si.q) || '%' THEN 250
                        WHEN be.title ILIKE '%' || si.q || '%' THEN 100
                        ELSE 0
                    END AS title_score
                FROM base_entries be
                CROSS JOIN search_input si
                WHERE be.title ILIKE '%' || si.q || '%'
            ),
            author_hits AS (
                SELECT
                    be.id,
                    50 AS author_score
                FROM base_entries be
                JOIN content_entry_authors cea ON cea.entry_id = be.id
                JOIN content_authors ca ON ca.id = cea.author_id
                CROSS JOIN search_input si
                WHERE (
                    ca.first_name ILIKE '%' || si.q || '%'
                    OR ca.last_name ILIKE '%' || si.q || '%'
                )
                GROUP BY be.id
            ),
            fulltext_hits AS (
                SELECT
                    be.id,
                    MAX(COALESCE(ts_rank_cd(cn.text_vector, lq.phrase_q), 0)) AS phrase_rank,
                    MAX(COALESCE(ts_rank_cd(cn.text_vector, lq.phrase_prefix_q), 0)) AS phrase_prefix_rank,
                    MAX(COALESCE(ts_rank_cd(cn.text_vector, lq.prefix_q), 0)) AS prefix_rank
                FROM base_entries be
                JOIN locale_queries lq ON lq.locale_id = be.locale_id
                JOIN content_nodes cn ON cn.entry_id = be.id
                WHERE (
                    cn.text_vector @@ lq.phrase_q
                    OR (
                        lq.phrase_prefix_q IS NOT NULL
                        AND cn.text_vector @@ lq.phrase_prefix_q
                    )
                    OR (
                        lq.prefix_q IS NOT NULL
                        AND cn.text_vector @@ lq.prefix_q
                    )
                )
                GROUP BY be.id
            ),
            filtered AS NOT MATERIALIZED (
                SELECT
                    be.*,
                    (
                        COALESCE(th.title_score, 0)
                        + COALESCE(ah.author_score, 0)
                        + COALESCE(fh.phrase_rank, 0) * 50
                        + COALESCE(fh.phrase_prefix_rank, 0) * 25
                        + COALESCE(fh.prefix_rank, 0) * 5
                    ) AS search_score
                FROM base_entries be
                LEFT JOIN title_hits th ON th.id = be.id
                LEFT JOIN author_hits ah ON ah.id = be.id
                LEFT JOIN fulltext_hits fh ON fh.id = be.id
                WHERE th.id IS NOT NULL OR ah.id IS NOT NULL OR fh.id IS NOT NULL"#,
        );
    }

    if query_obj.grouped {
        qb.push(
            " ), page AS ( SELECT * FROM ( SELECT DISTINCT ON (f.group_id) f.* FROM filtered f ORDER BY f.group_id,",
        );

        if let Some(code) = &query_obj.locale_code {
            qb.push("(f.locale_slug = ");
            qb.push_bind(code);
            qb.push(") DESC,");
        }

        qb.push(" f.created_at DESC ) picked ORDER BY");
    } else {
        qb.push(" ), page AS ( SELECT f.* FROM filtered f ORDER BY");
    };
    if query_obj.search.is_some() {
        if query_obj.grouped {
            if picked_ordering.is_empty() {
                qb.push(" picked.search_score DESC, picked.id DESC");
            } else {
                qb.push(format!(" picked.search_score DESC, {}", picked_ordering));
            }
        } else if page_ordering.is_empty() {
            qb.push(" f.search_score DESC, f.id DESC");
        } else {
            qb.push(format!(" f.search_score DESC, {}", page_ordering));
        }
    } else if query_obj.grouped {
        if picked_ordering.is_empty() {
            qb.push(" picked.created_at DESC");
        } else {
            qb.push(format!(" {}", picked_ordering));
        }
    } else if !page_ordering.is_empty() {
        qb.push(format!(" {}", page_ordering));
    }
    qb.push(format!(
        " LIMIT {} OFFSET {}",
        query_obj.limit, query_obj.offset
    ));

    if query_obj.search_slug.is_none() {
        match query_obj.grouped {
            true => qb.push(
                " ), total AS ( SELECT COUNT(DISTINCT group_id) AS total_count FROM filtered ) SELECT ",
            ),
            false => {
                qb.push(" ), total AS ( SELECT COUNT(*) AS total_count FROM filtered ) SELECT ")
            }
        };
    } else {
        qb.push(" ) SELECT ");
    }

    let mut sep = qb.separated(", ");
    let mut add_author = true;
    let mut add_category = true;
    let mut add_node = true;

    #[cfg(debug_assertions)]
    let timer = std::time::Instant::now();

    for f in &query_obj.fields {
        match *f {
            CF::Author(_) => {
                if add_author {
                    add_author = false;
                    sep.push("COALESCE(authors.data, '[]'::jsonb) AS authors".to_string())
                } else {
                    continue;
                }
            }
            CF::Category(_) => {
                if add_category {
                    add_category = false;
                    sep.push("COALESCE(cats.data, NULL) AS category".to_string())
                } else {
                    continue;
                }
            }
            CF::Tags => sep.push(format!("COALESCE(tags.data, ARRAY[]::record[]) AS {f}")),
            CF::Type => sep.push("(ct.id, ct.name, ct.slug, ct.order_index, ct.use_meta) AS type".to_string()),
            CF::Meta => sep.push(format!("(cm.start_time, cm.end_time) AS {f}")),
            CF::CommentCount => {
                sep.push("COALESCE(cc.comment_count, 0) AS comment_count".to_string())
            }
            CF::Node(_) => {
                if add_node {
                    add_node = false;
                    sep.push("COALESCE(nodes, '[]'::jsonb) AS nodes".to_string())
                } else {
                    continue;
                }
            }
            CF::GroupMembers => sep.push(format!("COALESCE(group_members.data, '[]') AS {f}")),
            CF::Media => sep.push("media.data AS \"media\""),
            _ => sep.push(format!("p.{f}")),
        };
    }

    if query_obj.search_slug.is_none() {
        sep.push("t.total_count");
        sep.push_unseparated(" ");
        qb.push("FROM page p CROSS JOIN total t ");
    } else {
        qb.push(" FROM page p ");
    }

    if query_obj.fields.contains(&CF::Type) {
        qb.push("LEFT JOIN content_types ct ON ct.id = p.type_id ");
    }

    if query_obj.fields.contains(&CF::CommentCount) {
        qb.push(comment_count_join("p"));
    }

    if query_obj.fields.iter().any(|f| matches!(f, CF::Author(_))) {
        qb.push(authors_join(query_obj, "p", false));
    }

    if query_obj
        .fields
        .iter()
        .any(|f| matches!(f, CF::Category(_)))
    {
        qb.push(category_join(query_obj, "p"));
    }

    if query_obj.fields.contains(&CF::Tags) {
        qb.push(tag_join("p"));
    }

    if query_obj.fields.contains(&CF::Meta) {
        qb.push("LEFT JOIN content_meta cm ON cm.entry_id = p.id ");
    }

    if query_obj.fields.contains(&CF::Media) {
        qb.push(media_join("p"));
    }

    if query_obj.fields.iter().any(|f| matches!(f, CF::Node(_))) {
        qb.push(nodes_join(query_obj, "p"));
    }

    if query_obj.fields.contains(&CF::GroupMembers) {
        qb.push(group_join("p"));
    }

    if query_obj.search.is_some() {
        if outer_ordering.is_empty() {
            qb.push(" ORDER BY p.search_score DESC, p.id DESC");
        } else {
            qb.push(format!(" ORDER BY p.search_score DESC, {}", outer_ordering));
        }
    } else if query_obj.grouped && outer_ordering.is_empty() {
        qb.push(" ORDER BY p.created_at DESC");
    } else if !outer_ordering.is_empty() {
        qb.push(format!(" ORDER BY {}", outer_ordering));
    }

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(qb.sql()));

    let query = qb.build_query_as::<ContentEntrySerializer>();

    let mut data: Vec<ContentEntrySerializer> = query.fetch_all(pool).await?;

    if query_obj.search_slug.is_some() && data.len() == 1 {
        data[0].total_count = Some(1);
    }

    #[cfg(debug_assertions)]
    debug!(
        "{}",
        format!("--> Selection time: {:?}", timer.elapsed()).bright_black()
    );

    Ok(RespondObj::new(query_obj, data))
}

pub async fn delete_author_from_entry(
    pool: &PgPool,
    entry_id: i32,
    author_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM content_entry_authors WHERE entry_id = $1 AND author_id = $2")
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
    node_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM content_node_media WHERE node_id = $1")
        .bind(node_id)
        .execute(pool)
        .await
        .map(|_| ())
}

pub async fn select_entry_text(pool: &PgPool, node_id: i64) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>("SELECT text FROM content_nodes WHERE id = $1")
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
    let existing_nodes: Vec<ContentNodeRecord> = sqlx::query_as(
        "SELECT id, order_index, name, text, data FROM content_nodes WHERE entry_id = $1 ORDER BY order_index",
    )
    .bind(entry_id)
    .fetch_all(pool)
    .await?;

    let mut existing_ids = Vec::new();
    let mut order_index = 1;

    // TODO: work with transactions: let mut tx = pool.begin().await?;

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
                let node_name = block_obj.get("name").and_then(Value::as_str);
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
                        .execute(pool)
                        .await?;

                        // Set parent_id if this is the first node in the block
                        if parent_id.is_none() {
                            parent_id = Some(id);
                        }
                    }
                    None => {
                        // Insert new node
                        let new_node_id: i64 = sqlx::query_scalar(
                            "INSERT INTO content_nodes (entry_id, media_id, order_index, name, data, parent_id) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
                        )
                        .bind(entry_id)
                        .bind(media_id)
                        .bind(order_index)
                        .bind(node_name)
                        .bind(&data)
                        .bind(parent_id)
                        .fetch_one(pool)
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
            let name = node_obj.get("name").and_then(|t| t.as_str());
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
                    .execute(pool)
                    .await?;

                    // Process text and media
                    if let Some(text_str) = text
                        && !text_str.is_empty()
                    {
                        let ast = to_mdast(text_str, &ParseOptions::default())?;
                        let tree: Value = serde_json::to_value(ast).unwrap_or_default();
                        delete_content_media_for_entry(pool, entry_id).await?;
                        persist_content_media(pool, id, &tree).await?;
                    }
                }
                None => {
                    // Insert new node
                    let new_node_id: i64 = sqlx::query_scalar(
                        "INSERT INTO content_nodes (entry_id, media_id, order_index, name, text, data) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
                    )
                    .bind(entry_id)
                    .bind(media_id)
                    .bind(order_index)
                    .bind(name)
                    .bind(text)
                    .bind(&data)
                    .fetch_one(pool)
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
    for (db_id, _, _, _, _) in &existing_nodes {
        if !existing_ids.contains(db_id) {
            sqlx::query("DELETE FROM content_nodes WHERE id = $1")
                .bind(db_id)
                .execute(pool)
                .await?;
        }
    }

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
