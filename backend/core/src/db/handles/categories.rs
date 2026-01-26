use sqlx::{Postgres, QueryBuilder, postgres::PgPool};
use strum::IntoEnumIterator;

#[cfg(debug_assertions)]
use sqlx::Execute;
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
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut sep = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            CCF::Media => sep.push("COALESCE(media.data, NULL) AS \"media\""),
            CCF::GroupMembers => sep.push(format!("COALESCE(group_members.data, '[]') AS {f}")),
            _ => sep.push(format!("cc.{f}")),
        };
    }

    sep.push("count(*) OVER() AS total_count");
    sep.push_unseparated(" ");
    query_builder.push("FROM content_categories cc ");

    if query_obj.fields.contains(&CCF::LocaleID) || query_obj.search_locale.is_some() {
        query_builder.push("LEFT JOIN locales l ON l.id = cc.locale_id ");
    }

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
                            )
                            FROM media_variants mv
                            WHERE mv.media_id = m.id
                        ),
                        '[]'
                    )
                ) AS data
                FROM media m
                WHERE m.id = cc.media_id
            ) AS media ON TRUE "#,
        );
    }

    if query_obj.fields.contains(&CCF::GroupMembers) {
        query_builder.push(
            r#"LEFT JOIN LATERAL (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', ge.id,
                        'locale_id', ge.locale_id
                    )
                ) AS data
                FROM content_entries ge
                WHERE ge.group_id = cc.group_id
                  AND ge.id != cc.id
            ) AS group_members ON TRUE "#,
        );
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

    // take builder back from where_chain
    query_builder = where_chain.into_inner();

    let ordering = query_obj
        .ordering
        .split(',')
        .filter_map(|item| {
            let item = item.trim();
            if CCF::iter().any(|f| item.contains(&f.to_string())) {
                Some(format!("cc.{item}"))
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

    let query = query_builder.build_query_as::<ContentCategorySerializer>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()));

    let data: Vec<ContentCategorySerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(query_obj, data))
}
