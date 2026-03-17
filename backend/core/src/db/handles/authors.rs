use sqlx::{Postgres, QueryBuilder, postgres::PgPool};
use strum::IntoEnumIterator;

#[cfg(debug_assertions)]
use sqlx::Execute;
#[cfg(debug_assertions)]
use tracing::debug;

use crate::db::{
    fields::ContentAuthorFields,
    queries::{QueryObj, RespondObj, WhereBuilder},
    serialize::AuthorSerializer,
};
use crate::utils::errors::NurError;

#[cfg(debug_assertions)]
use crate::db::format_sql;

pub async fn select_content_author(
    pool: &PgPool,
    query_obj: QueryObj<ContentAuthorFields>,
) -> Result<RespondObj<AuthorSerializer>, NurError> {
    let ordering_with_alias = |alias: &str| {
        query_obj
            .ordering
            .split(',')
            .filter_map(|part| {
                let mut split = part.split_whitespace();
                let column = split.next()?.trim();
                let direction = split.next().unwrap_or("ASC").to_uppercase();

                if direction != "ASC" && direction != "DESC" {
                    return None;
                }

                if ContentAuthorFields::iter()
                    .any(|f| f.to_string() == column && !matches!(f, ContentAuthorFields::Media))
                {
                    Some(format!("{alias}.{column} {direction}"))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    let page_ordering = ordering_with_alias("f");
    let outer_ordering = ordering_with_alias("p");

    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new(
        "WITH filtered AS NOT MATERIALIZED ( SELECT ca.* FROM content_authors ca ",
    );

    let mut where_chain = WhereBuilder::new(query_builder);

    if let Some(id) = &query_obj.search_id {
        where_chain.push_and_bind(None, "ca.id = ", id, None);
    }

    if let Some(after) = &query_obj.created_after {
        where_chain.push_and_bind(None, "ca.created_at >= ", after, None);
    }

    if let Some(before) = &query_obj.created_before {
        where_chain.push_and_bind(None, "ca.created_at < ", before, None);
    }

    if let Some(search) = query_obj.search.clone() {
        where_chain.push_and_bind(
            None,
            "(ca.first_name ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );

        where_chain.push_and_bind(
            Some(" OR"),
            "ca.last_name ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%'))"),
        );
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

    let mut separated = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            ContentAuthorFields::Media => separated.push("media.data AS \"media\""),
            ContentAuthorFields::MediaID => {
                if !query_obj.fields.contains(&ContentAuthorFields::Media) {
                    separated.push(format!("p.{f}"));
                }
                continue;
            }
            _ => separated.push(format!("p.{f}")),
        };
    }

    separated.push("t.total_count");
    separated.push_unseparated(" ");
    query_builder.push("FROM page p CROSS JOIN total t ");

    if query_obj.fields.contains(&ContentAuthorFields::Media) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT json_build_object(
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

    if !outer_ordering.is_empty() {
        query_builder.push(format!(" ORDER BY {}", outer_ordering));
    }

    let query = query_builder.build_query_as::<AuthorSerializer>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()));

    let data: Vec<AuthorSerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(&query_obj, data))
}
