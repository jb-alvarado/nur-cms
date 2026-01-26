use sqlx::{Postgres, QueryBuilder, postgres::PgPool};

#[cfg(debug_assertions)]
use sqlx::Execute;
#[cfg(debug_assertions)]
use tracing::debug;

use crate::db::{
    fields::AuthUserFields,
    queries::{QueryObj, RespondObj, WhereBuilder},
    serialize::AuthUserSerializer,
};
use crate::utils::errors::NurError;

#[cfg(debug_assertions)]
use crate::db::format_sql;

pub async fn select_auth_user(
    pool: &PgPool,
    query_obj: QueryObj<AuthUserFields>,
) -> Result<RespondObj<AuthUserSerializer>, NurError> {
    let mut query_builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut separated = query_builder.separated(", ");

    for f in &query_obj.fields {
        match *f {
            AuthUserFields::Role => separated.push("(r.id, r.name) AS \"auth_role\""),
            _ => separated.push(format!("u.{f}")),
        };
    }

    separated.push("count(*) OVER() AS total_count");

    separated.push_unseparated(" ");
    query_builder.push("FROM auth_users u ");

    if query_obj.fields.contains(&AuthUserFields::Role) {
        query_builder.push("LEFT JOIN auth_roles r ON r.id = u.role_id");
    }

    let mut where_chain = WhereBuilder::new(query_builder);

    if let Some(id) = &query_obj.search_id {
        where_chain.push_and_bind(None, "u.id = ", id, None);
    }

    if let Some(after) = &query_obj.created_after {
        where_chain.push_and_bind(None, "u.created_at >= ", after, None);
    }

    if let Some(before) = &query_obj.created_before {
        where_chain.push_and_bind(None, "u.created_at < ", before, None);
    }

    if let Some(search) = query_obj.search.clone() {
        where_chain.push_and_bind(
            None,
            "u.username ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );

        where_chain.push_and_bind(
            Some(" OR"),
            "u.email ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
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

    let query = query_builder.build_query_as::<AuthUserSerializer>();

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query.sql()));

    let data: Vec<AuthUserSerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(&query_obj, data))
}
