use sqlx::{Postgres, QueryBuilder, postgres::PgPool};
use strum::IntoEnumIterator;
use tracing::debug;

use crate::db::{
    fields::MediaFields,
    queries::{QueryObj, RespondObj, WhereBuilder},
    serialize::MediaSerializer,
};
use crate::utils::errors::ServiceError;

#[cfg(debug_assertions)]
use crate::db::format_sql;
#[cfg(debug_assertions)]
use sqlx::Execute;

pub async fn select_media(
    pool: &PgPool,
    query_obj: &QueryObj<MediaFields>,
) -> Result<RespondObj<MediaSerializer>, ServiceError> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut sep = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            MediaFields::MediaVariants => sep.push("COALESCE(variants.data, NULL) AS \"variants\""),
            _ => sep.push(format!("m.{f}")),
        };
    }

    sep.push("count(*) OVER() AS total_count");
    sep.push_unseparated(" ");
    query_builder.push("FROM media m ");
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
                ) AS data
                FROM media_variants mv
                WHERE mv.media_id = m.id
            ) AS variants ON TRUE "#,
        );
    }

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

    if !query_obj.media_type.is_empty() {
        let array: Vec<String> = query_obj
            .media_type
            .iter()
            .map(|t| format!("{t}/%"))
            .collect();
        where_chain.push_and_bind(None, "m.type LIKE ANY(", array, Some(")"));
    }

    // take builder back from where_chain
    query_builder = where_chain.into_inner();

    let ordering = query_obj
        .ordering
        .split(',')
        .filter_map(|item| {
            let item = item.trim();
            if MediaFields::iter().any(|f| item.contains(&f.to_string())) {
                Some(format!("m.{item}"))
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

    let query = query_builder.build_query_as::<MediaSerializer>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()));

    let data: Vec<MediaSerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(query_obj, data))
}
