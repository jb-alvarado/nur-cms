use sqlx::{Postgres, QueryBuilder, postgres::PgPool};
use tracing::debug;

use crate::db::{
    fields::ContentAuthorFields,
    queries::{QueryObj, RespondObj, WhereBuilder},
    serialize::AuthorSerializer,
};
use crate::utils::errors::ServiceError;

#[cfg(debug_assertions)]
use crate::db::format_sql;
#[cfg(debug_assertions)]
use sqlx::Execute;

pub async fn select_content_author(
    pool: &PgPool,
    query_obj: QueryObj<ContentAuthorFields>,
) -> Result<RespondObj<AuthorSerializer>, ServiceError> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut separated = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            ContentAuthorFields::Media => {
                separated.push("COALESCE(media.data, '{}'::json) AS \"media\"")
            }
            ContentAuthorFields::MediaID => {
                if !query_obj.fields.contains(&ContentAuthorFields::Media) {
                    separated.push(format!("ca.{f}"));
                }
                continue;
            }
            _ => separated.push(format!("ca.{f}")),
        };
    }

    separated.push("count(*) OVER() AS total_count");

    separated.push_unseparated(" ");
    query_builder.push("FROM content_authors ca ");

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
                            )
                            FROM media_variants mv
                            WHERE mv.media_id = m.id
                        ),
                        '[]'
                    )
                ) AS data
                FROM media m
                WHERE m.id = ca.media_id
            ) AS media ON TRUE "#,
        );
    }

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

    let ordering: Vec<String> = query_obj
        .ordering
        .split(',')
        .filter_map(|part| {
            let mut split = part.split_whitespace();
            let column = split.next()?;
            let direction = split.next().unwrap_or("ASC").to_uppercase();

            if query_obj.fields.iter().any(|f| f.to_string() == column)
                && (direction == "ASC" || direction == "DESC")
            {
                Some(format!("{column} {direction}"))
            } else {
                None
            }
        })
        .collect();

    if !ordering.is_empty() {
        query_builder.push(" ORDER BY ");
        query_builder.push(ordering.join(", "));
    }

    query_builder.push(format!(
        " LIMIT {} OFFSET {}",
        query_obj.limit, query_obj.offset
    ));

    let query = query_builder.build_query_as::<AuthorSerializer>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()));

    let data: Vec<AuthorSerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(&query_obj, data))
}
