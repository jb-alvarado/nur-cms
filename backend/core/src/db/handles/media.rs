use sqlx::{Postgres, QueryBuilder, postgres::PgPool};
use strum::IntoEnumIterator;

#[cfg(debug_assertions)]
use tracing::debug;

use crate::db::{
    fields::MediaFields,
    queries::{QueryObj, RespondObj, WhereBuilder},
    serialize::MediaSerializer,
};
use crate::utils::errors::NurError;

#[cfg(debug_assertions)]
use crate::db::format_sql;

pub async fn select_media(
    pool: &PgPool,
    query_obj: &QueryObj<MediaFields>,
) -> Result<RespondObj<MediaSerializer>, NurError> {
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

                if MediaFields::iter()
                    .any(|f| f.to_string() == field && !matches!(f, MediaFields::MediaVariants))
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

    let mut query_builder: QueryBuilder<Postgres> =
        QueryBuilder::new("WITH filtered AS NOT MATERIALIZED ( SELECT m.* FROM media m ");

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

    if let Some(media_type) = &query_obj.media_type {
        let array: Vec<String> = media_type.iter().map(|t| format!("{t}/%")).collect();
        where_chain.push_and_bind(None, "m.type LIKE ANY(", array, Some(")"));
    }

    query_builder = where_chain.into_inner();

    query_builder.push(" ), page AS ( SELECT f.* FROM filtered f");

    if !page_ordering.is_empty() {
        query_builder.push(format!(" ORDER BY {}", page_ordering));
    }

    query_builder.push(format!(
        " LIMIT {} OFFSET {}",
        query_obj.limit, query_obj.offset
    ));

    query_builder.push(" ), total AS ( SELECT COUNT(*) AS total_count FROM filtered ) SELECT ");

    let mut sep = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            MediaFields::MediaVariants => sep.push("COALESCE(variants.data, NULL) AS \"variants\""),
            _ => sep.push(format!("p.{f}")),
        };
    }

    sep.push("t.total_count");
    sep.push_unseparated(" ");
    query_builder.push("FROM page p CROSS JOIN total t ");

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
                    ORDER BY mv.id
                ) AS data
                FROM media_variants mv
                WHERE mv.media_id = p.id
            ) AS variants ON TRUE "#,
        );
    }

    if !outer_ordering.is_empty() {
        query_builder.push(format!(" ORDER BY {}", outer_ordering));
    }

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query_builder.sql()));

    let query = query_builder.build_query_as::<MediaSerializer>();

    let data: Vec<MediaSerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(query_obj, data))
}
