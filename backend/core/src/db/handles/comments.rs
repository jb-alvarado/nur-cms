use chrono::{DateTime, Utc};
use sqlx::{Postgres, QueryBuilder, Row, postgres::PgPool};
use strum::IntoEnumIterator;

#[cfg(debug_assertions)]
use tracing::debug;

use crate::db::{
    fields::CommentFields,
    models::Comment,
    queries::{QueryObj, RespondObj, WhereBuilder, parse_ordering},
};
use crate::utils::errors::NurError;

#[cfg(debug_assertions)]
use crate::db::format_sql;

fn comment_ordering(ordering: &str) -> String {
    parse_ordering(ordering)
        .split(',')
        .filter_map(|item| {
            let mut parts = item.split_whitespace();
            let column = parts.next()?;
            let direction = parts.next()?;

            if CommentFields::iter()
                .any(|field| field.to_string() == column && !matches!(field, CommentFields::Entry))
            {
                Some(format!("c.{column} {direction}"))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub async fn select_comments(
    pool: &PgPool,
    query_obj: &QueryObj<CommentFields>,
) -> Result<RespondObj<Comment>, NurError> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new("SELECT ");
    let mut sep = qb.separated(", ");

    for f in &query_obj.fields {
        if *f == CommentFields::Entry {
            sep.push("(e.id, e.title, t.slug, e.slug) AS entry".to_string());
        } else {
            sep.push(format!("c.{f}"));
        }
    }

    sep.push("count(*) OVER() AS total_count");
    sep.push_unseparated(" ");
    qb.push("FROM comments c ");

    if query_obj.fields.contains(&CommentFields::Entry) || query_obj.search_slug.is_some() {
        qb.push(
            "LEFT JOIN content_entries e ON e.id = c.entry_id
            LEFT JOIN content_types t ON t.id = e.type_id ",
        );
    }

    let mut where_chain = WhereBuilder::new(qb);

    if let Some(id) = &query_obj.search_id {
        where_chain.push_and_bind(None, "c.id = ", id, None);
    }

    if let Some(id) = &query_obj.entry_id {
        where_chain.push_and_bind(None, "c.entry_id = ", id, None);
    }

    if let Some(status) = &query_obj.search_status {
        where_chain.push_and_bind(None, "c.status = ", status, None);
    }

    if let Some(slug) = &query_obj.search_slug {
        where_chain.push_and_bind(None, "e.slug = ", slug, None);
    }

    if let Some(search) = &query_obj.search {
        where_chain.push_and_bind(
            None,
            "c.author_name ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );
        where_chain.push_and_bind(
            Some("OR"),
            "c.author_email ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );
        where_chain.push_and_bind(
            Some("OR"),
            "c.text ILIKE CONCAT('%', ",
            search.clone(),
            Some(", '%')"),
        );
    }

    // take builder back from where_chain
    qb = where_chain.into_inner();

    let ordering = comment_ordering(&query_obj.ordering);
    if !ordering.is_empty() {
        qb.push(format!(" ORDER BY {}", ordering));
    }

    qb.push(format!(
        " LIMIT {} OFFSET {}",
        query_obj.limit, query_obj.offset
    ));

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(qb.sql()));

    let query = qb.build_query_as::<Comment>();

    let data: Vec<Comment> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(query_obj, data))
}

pub async fn insert_comment(pool: &PgPool, c: &Comment) -> Result<i64, NurError> {
    let entry_id = c.entry_id.ok_or(NurError::InvalidInput)?;
    let text = c.text.as_deref().ok_or(NurError::InvalidInput)?;
    let status = c.status.as_deref().unwrap_or("pending");
    let mut qb = QueryBuilder::<Postgres>::new("INSERT INTO comments (");
    let mut keys = vec!["entry_id", "text", "status"];

    if c.parent_id.is_some() {
        keys.push("parent_id");
    }

    if c.author_email.is_some() {
        keys.push("author_email");
    }

    if c.author_name.is_some() {
        keys.push("author_name");
    }

    if c.user_id.is_some() && c.author_email.is_none() {
        keys.push("user_id");
    }

    if c.user_id.is_some() && c.created_at.is_some() {
        keys.push("created_at");
    }

    if c.user_id.is_some() && c.updated_at.is_some() {
        keys.push("updated_at");
    }

    qb.push(keys.join(", "));
    qb.push(") VALUES (");

    let mut separated = qb.separated(", ");
    separated.push_bind(entry_id);
    separated.push_bind(text);
    separated.push_bind(status);

    if let Some(parent_id) = c.parent_id {
        separated.push_bind(parent_id);
    }

    if let Some(author_email) = c.author_email.as_deref() {
        separated.push_bind(author_email);
    }

    if let Some(author_name) = c.author_name.as_deref() {
        separated.push_bind(author_name);
    }

    if let Some(user_id) = c.user_id
        && c.author_email.is_none()
    {
        separated.push_bind(user_id);
    }

    if c.user_id.is_some() {
        if let Some(created_at) = c.created_at {
            separated.push_bind(created_at);
        }
        if let Some(updated_at) = c.updated_at {
            separated.push_bind(updated_at);
        }
    }

    qb.push(") RETURNING id");

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(qb.sql()));

    let query = qb.build_query_scalar();

    let id = query.fetch_one(pool).await?;

    Ok(id)
}

pub async fn insert_comment_moderation_tokens(
    pool: &PgPool,
    comment_id: i64,
    approve_token_hash: &[u8],
    reject_token_hash: &[u8],
    expires_at: DateTime<Utc>,
) -> Result<(), NurError> {
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "DELETE FROM comment_moderation_tokens WHERE used_at IS NOT NULL OR expires_at <= now()",
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO comment_moderation_tokens (comment_id, token_hash, action, expires_at)
         VALUES ($1, $2, 'approved', $4), ($1, $3, 'rejected', $4)",
    )
    .bind(comment_id)
    .bind(approve_token_hash)
    .bind(reject_token_hash)
    .bind(expires_at)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn select_comment_moderation_action(
    pool: &PgPool,
    token_hash: &[u8],
) -> Result<Option<String>, NurError> {
    Ok(sqlx::query_scalar(
        "SELECT token.action
         FROM comment_moderation_tokens token
         INNER JOIN comments comment ON comment.id = token.comment_id
         WHERE token.token_hash = $1
           AND token.used_at IS NULL
           AND token.expires_at > now()
           AND comment.status = 'pending'",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?)
}

pub async fn consume_comment_moderation_token(
    pool: &PgPool,
    token_hash: &[u8],
) -> Result<Option<String>, NurError> {
    let mut transaction = pool.begin().await?;
    let token = sqlx::query(
        "SELECT comment_id
         FROM comment_moderation_tokens
         WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now()",
    )
    .bind(token_hash)
    .fetch_optional(&mut *transaction)
    .await?;

    let Some(token) = token else {
        transaction.commit().await?;
        return Ok(None);
    };
    let comment_id: i64 = token.try_get("comment_id")?;
    sqlx::query("SELECT id FROM comments WHERE id = $1 FOR UPDATE")
        .bind(comment_id)
        .fetch_optional(&mut *transaction)
        .await?;

    let action: Option<String> = sqlx::query_scalar(
        "SELECT action
         FROM comment_moderation_tokens
         WHERE token_hash = $1
           AND comment_id = $2
           AND used_at IS NULL
           AND expires_at > now()",
    )
    .bind(token_hash)
    .bind(comment_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(action) = action else {
        transaction.commit().await?;
        return Ok(None);
    };
    let updated = sqlx::query(
        "UPDATE comments
         SET status = $1, updated_at = now()
         WHERE id = $2 AND status = 'pending'",
    )
    .bind(&action)
    .bind(comment_id)
    .execute(&mut *transaction)
    .await?;

    sqlx::query(
        "UPDATE comment_moderation_tokens
         SET used_at = now()
         WHERE comment_id = $1 AND used_at IS NULL",
    )
    .bind(comment_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok((updated.rows_affected() == 1).then_some(action))
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use sqlx::PgPool;

    use super::{
        comment_ordering, consume_comment_moderation_token, insert_comment_moderation_tokens,
        select_comment_moderation_action,
    };

    const MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

    async fn insert_pending_comment(pool: &PgPool) -> i64 {
        let entry_id: i32 = sqlx::query_scalar(
            "INSERT INTO content_entries (type_id, locale_id, slug, title, status)
             VALUES (
                 (SELECT id FROM content_types WHERE slug = 'article'),
                 (SELECT id FROM locales WHERE code = 'en'),
                 'moderation-test-entry',
                 'Moderation test entry',
                 'published'
             )
             RETURNING id",
        )
        .fetch_one(pool)
        .await
        .expect("test entry can be inserted");

        sqlx::query_scalar(
            "INSERT INTO comments (entry_id, author_name, author_email, text, status)
             VALUES ($1, 'Commenter', 'commenter@example.org', 'Pending comment', 'pending')
             RETURNING id",
        )
        .bind(entry_id)
        .fetch_one(pool)
        .await
        .expect("test comment can be inserted")
    }

    #[test]
    fn normalizes_descending_comment_ordering() {
        assert_eq!(comment_ordering("-created_at"), "c.created_at DESC");
        assert_eq!(
            comment_ordering("created_at DESC,id ASC"),
            "c.created_at DESC, c.id ASC"
        );
        assert!(comment_ordering("entry DESC").is_empty());
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn moderation_action_consumes_both_links(pool: PgPool) {
        let comment_id = insert_pending_comment(&pool).await;
        let approve_hash = b"approve-token-hash";
        let reject_hash = b"reject-token-hash";
        insert_comment_moderation_tokens(
            &pool,
            comment_id,
            approve_hash,
            reject_hash,
            Utc::now() + Duration::days(14),
        )
        .await
        .expect("moderation tokens can be inserted");

        assert_eq!(
            select_comment_moderation_action(&pool, approve_hash)
                .await
                .expect("approve token can be selected")
                .as_deref(),
            Some("approved")
        );
        assert_eq!(
            consume_comment_moderation_token(&pool, approve_hash)
                .await
                .expect("approve token can be consumed")
                .as_deref(),
            Some("approved")
        );
        assert!(
            consume_comment_moderation_token(&pool, reject_hash)
                .await
                .expect("reject token lookup succeeds")
                .is_none()
        );

        let status: String = sqlx::query_scalar("SELECT status FROM comments WHERE id = $1")
            .bind(comment_id)
            .fetch_one(&pool)
            .await
            .expect("comment status can be selected");
        assert_eq!(status, "approved");
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn concurrent_moderation_actions_do_not_deadlock(pool: PgPool) {
        let comment_id = insert_pending_comment(&pool).await;
        let approve_hash = b"concurrent-approve-hash";
        let reject_hash = b"concurrent-reject-hash";
        insert_comment_moderation_tokens(
            &pool,
            comment_id,
            approve_hash,
            reject_hash,
            Utc::now() + Duration::days(14),
        )
        .await
        .expect("moderation tokens can be inserted");

        let (approve, reject) = tokio::join!(
            consume_comment_moderation_token(&pool, approve_hash),
            consume_comment_moderation_token(&pool, reject_hash),
        );
        let approve = approve.expect("concurrent approve request succeeds");
        let reject = reject.expect("concurrent reject request succeeds");

        assert_ne!(approve.is_some(), reject.is_some());
        let expected_status = approve
            .as_deref()
            .or(reject.as_deref())
            .expect("one action wins");
        let status: String = sqlx::query_scalar("SELECT status FROM comments WHERE id = $1")
            .bind(comment_id)
            .fetch_one(&pool)
            .await
            .expect("comment status can be selected");
        assert_eq!(status, expected_status);
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn expired_moderation_links_are_rejected(pool: PgPool) {
        let comment_id = insert_pending_comment(&pool).await;
        let approve_hash = b"expired-approve-hash";
        insert_comment_moderation_tokens(
            &pool,
            comment_id,
            approve_hash,
            b"expired-reject-hash",
            Utc::now() - Duration::seconds(1),
        )
        .await
        .expect("expired test tokens can be inserted");

        assert!(
            select_comment_moderation_action(&pool, approve_hash)
                .await
                .expect("expired token lookup succeeds")
                .is_none()
        );
        assert!(
            consume_comment_moderation_token(&pool, approve_hash)
                .await
                .expect("expired token consumption succeeds")
                .is_none()
        );
    }
}
