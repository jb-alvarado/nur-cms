use sqlx::{Postgres, QueryBuilder, postgres::PgPool};
use strum::IntoEnumIterator;

#[cfg(debug_assertions)]
use tracing::debug;

use crate::db::{
    fields::ContentCategoryFields as CCF,
    queries::{QueryObj, RespondObj, WhereBuilder},
    serialize::ContentCategorySerializer,
};
use crate::utils::errors::NurError;

#[cfg(debug_assertions)]
use crate::db::format_sql;

pub async fn select_categories(
    pool: &PgPool,
    query_obj: &QueryObj<CCF>,
) -> Result<RespondObj<ContentCategorySerializer>, NurError> {
    let ordering_with_alias = |alias: &str| {
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

                if CCF::iter()
                    .any(|f| f.to_string() == field && !matches!(f, CCF::Media | CCF::GroupMembers))
                {
                    Some(format!("{alias}.{field} {direction}"))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    let page_ordering = ordering_with_alias("f");
    let outer_ordering = ordering_with_alias("p");

    let lang = match query_obj.grouped {
        true => ", l.code AS locale_slug",
        false => "",
    };

    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new(format!(
        "WITH filtered AS NOT MATERIALIZED ( SELECT cc.*{lang} FROM content_categories cc "
    ));

    if query_obj.search_locale.is_some() || query_obj.grouped {
        query_builder.push("LEFT JOIN locales l ON l.id = cc.locale_id ");
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

    query_builder = where_chain.into_inner();

    if query_obj.grouped {
        query_builder.push(" ), page AS ( SELECT DISTINCT ON (f.group_id) f.* FROM filtered f ORDER BY f.group_id,");

        if let Some(code) = &query_obj.locale_code {
            query_builder.push("(f.locale_slug = ");
            query_builder.push_bind(code);
            query_builder.push(") DESC,");
        }
    } else {
        query_builder.push(" ), page AS ( SELECT f.* FROM filtered f");

        if !page_ordering.is_empty() {
            query_builder.push(" ORDER BY");
        }
    };

    if !page_ordering.is_empty() {
        query_builder.push(format!(" {}", page_ordering));
    }

    query_builder.push(format!(
        " LIMIT {} OFFSET {}",
        query_obj.limit, query_obj.offset
    ));

    match query_obj.grouped {
        true => query_builder.push(
            " ), total AS ( SELECT COUNT(DISTINCT group_id) AS total_count FROM filtered ) SELECT ",
        ),
        false => query_builder
            .push(" ), total AS ( SELECT COUNT(*) AS total_count FROM filtered ) SELECT "),
    };

    let mut sep = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            CCF::Media => sep.push("COALESCE(media.data, NULL) AS \"media\""),
            CCF::GroupMembers => sep.push(format!("COALESCE(group_members.data, '[]') AS {f}")),
            _ => sep.push(format!("p.{f}")),
        };
    }

    sep.push("t.total_count");
    sep.push_unseparated(" ");
    query_builder.push("FROM page p CROSS JOIN total t ");

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
                                ORDER BY mv.id
                            )
                            FROM media_variants mv
                            WHERE mv.media_id = m.id
                        ),
                        '[]'
                    )
                ) AS data
                FROM media m
                WHERE m.id = p.media_id
            ) AS media ON TRUE "#,
        );
    }

    if query_obj.fields.contains(&CCF::GroupMembers) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', ge.id,
                        'locale_code', l.code,
                        'locale_name', l.name
                    )
                ) AS data
                FROM content_categories ge
                JOIN locales l ON l.id = ge.locale_id
                WHERE ge.group_id = p.group_id
            ) AS group_members ON TRUE "#,
        );
    }

    if !outer_ordering.is_empty() {
        query_builder.push(format!(" ORDER BY {}", outer_ordering));
    }

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query_builder.sql()));

    let query = query_builder.build_query_as::<ContentCategorySerializer>();

    let data: Vec<ContentCategorySerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(query_obj, data))
}
