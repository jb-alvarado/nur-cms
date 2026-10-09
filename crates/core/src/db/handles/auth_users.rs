use sqlx::{Postgres, QueryBuilder, postgres::PgPool};

#[cfg(debug_assertions)]
use tracing::debug;

use crate::db::{
    fields::{AuthUserFields, Table},
    handles::{insert_record, update_record},
    models::AuthUser,
    queries::{QueryObj, RespondObj, WhereBuilder},
    serialize::AuthUserSerializer,
};
use crate::utils::errors::NurError;

const LOGIN_IDENTIFIER_CONFLICT: &str = "Username or email is already in use.";
pub(crate) const MAX_AUTH_PASSWORD_BYTES: usize = 1_024;

fn validate_password(password: &str, required: bool) -> Result<(), NurError> {
    if (required && password.is_empty()) || password.len() > MAX_AUTH_PASSWORD_BYTES {
        return Err(NurError::BadRequest(
            "Password must be 1–1024 bytes long.".into(),
        ));
    }

    Ok(())
}

async fn lock_login_identifiers(
    transaction: &mut sqlx::Transaction<'_, Postgres>,
) -> Result<(), NurError> {
    sqlx::query("SELECT pg_advisory_xact_lock(782413, 1)")
        .execute(&mut **transaction)
        .await?;

    Ok(())
}

async fn ensure_login_identifiers_available(
    transaction: &mut sqlx::Transaction<'_, Postgres>,
    username: &str,
    email: &str,
    current_id: Option<i32>,
) -> Result<(), NurError> {
    let conflict: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
               SELECT 1 FROM auth_users
               WHERE id IS DISTINCT FROM $3
                 AND (lower(username) IN (lower($1), lower($2))
                      OR lower(email) IN (lower($1), lower($2)))
           )"#,
    )
    .bind(username)
    .bind(email)
    .bind(current_id)
    .fetch_one(&mut **transaction)
    .await?;

    if conflict {
        return Err(NurError::Conflict(LOGIN_IDENTIFIER_CONFLICT.into()));
    }

    Ok(())
}

pub async fn insert_auth_user(pool: &PgPool, user: &AuthUser) -> Result<i32, NurError> {
    validate_password(&user.password, true)?;

    let mut transaction = pool.begin().await?;
    lock_login_identifiers(&mut transaction).await?;
    ensure_login_identifiers_available(&mut transaction, &user.username, &user.email, None).await?;

    let id = insert_record(&mut *transaction, &Table::AuthUsers, user).await?;
    transaction.commit().await?;

    Ok(id)
}

pub async fn update_auth_user(pool: &PgPool, id: i32, user: &AuthUser) -> Result<(), NurError> {
    validate_password(&user.password, false)?;

    let mut transaction = pool.begin().await?;
    lock_login_identifiers(&mut transaction).await?;

    let current: Option<(String, String)> =
        sqlx::query_as("SELECT username, email FROM auth_users WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await?;
    let (current_username, current_email) = current.ok_or(NurError::NotFound)?;
    let username = if user.username.is_empty() {
        &current_username
    } else {
        &user.username
    };
    let email = if user.email.is_empty() {
        &current_email
    } else {
        &user.email
    };

    if username != &current_username || email != &current_email {
        ensure_login_identifiers_available(&mut transaction, username, email, Some(id)).await?;
    }

    update_record(&mut *transaction, &Table::AuthUsers, id, user).await?;
    transaction.commit().await?;

    Ok(())
}

pub async fn update_last_login(pool: &PgPool, id: i32) -> Result<(), NurError> {
    sqlx::query("UPDATE auth_users SET last_login = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

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

    if query_obj.last_login {
        where_chain.push_and(None, "u.last_login IS NOT NULL");
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

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(query_builder.sql()));

    let query = query_builder.build_query_as::<AuthUserSerializer>();

    let data: Vec<AuthUserSerializer> = query.fetch_all(pool).await?;

    Ok(RespondObj::new(&query_obj, data))
}

pub async fn select_auth_user_for_login(
    pool: &PgPool,
    login: &str,
) -> Result<Option<AuthUserSerializer>, NurError> {
    let user = sqlx::query_as::<_, AuthUserSerializer>(
        r#"SELECT
               u.id,
               u.username,
               u.email,
               u.password,
               (r.id, r.name) AS "auth_role",
               1::bigint AS total_count
           FROM auth_users u
           JOIN auth_roles r ON r.id = u.role_id
           WHERE lower(u.username) = lower($1) OR lower(u.email) = lower($1)
           LIMIT 1"#,
    )
    .bind(login)
    .fetch_optional(pool)
    .await?;

    Ok(user)
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::{insert_auth_user, update_auth_user, validate_password};
    use crate::{db::models::AuthUser, utils::errors::NurError};

    const MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

    #[test]
    fn password_limits_match_login_for_insert_and_update() {
        assert!(validate_password("", true).is_err());
        assert!(validate_password("", false).is_ok());
        assert!(validate_password(&"a".repeat(1024), true).is_ok());
        assert!(validate_password(&"a".repeat(1025), true).is_err());
        assert!(validate_password(&"é".repeat(513), false).is_err());
    }

    async fn insert_user(pool: &PgPool, username: &str, email: &str) -> Result<i32, NurError> {
        let user = AuthUser::new(
            email.into(),
            username.into(),
            "Test".into(),
            "User".into(),
            "password".into(),
            3,
        );

        insert_auth_user(pool, &user).await
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn login_identifiers_are_unique_across_columns_and_case(pool: PgPool) {
        let id = insert_user(&pool, "Alice", "alice@example.org")
            .await
            .expect("first user should be inserted");

        for (username, email) in [
            ("ALICE", "other@example.org"),
            ("other", "ALICE@EXAMPLE.ORG"),
            ("alice@example.org", "third@example.org"),
            ("third", "ALICE"),
        ] {
            let error = insert_user(&pool, username, email)
                .await
                .expect_err("ambiguous identifier should be rejected");
            assert!(matches!(error, NurError::Conflict(_)));
        }

        let user = AuthUser {
            username: "ALICE@EXAMPLE.ORG".into(),
            ..Default::default()
        };
        update_auth_user(&pool, id, &user)
            .await
            .expect("a user may use their own email as username");

        insert_user(&pool, "bob", "bob@example.org")
            .await
            .expect("second user should be inserted");
        let user = AuthUser {
            username: "BOB".into(),
            ..Default::default()
        };
        let error = update_auth_user(&pool, id, &user)
            .await
            .expect_err("update must reject another user's identifier");
        assert!(matches!(error, NurError::Conflict(_)));
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn concurrent_inserts_cannot_claim_the_same_login(pool: PgPool) {
        let (first, second) = tokio::join!(
            insert_user(&pool, "shared@example.org", "first@example.org"),
            insert_user(&pool, "second", "SHARED@EXAMPLE.ORG"),
        );

        let (success, failure) = match (first, second) {
            (Ok(id), Err(error)) | (Err(error), Ok(id)) => (id, error),
            (first, second) => {
                panic!("expected one successful insert and one conflict: {first:?}, {second:?}")
            }
        };

        assert!(success > 0);
        assert!(matches!(failure, NurError::Conflict(_)));
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn unchanged_legacy_identifiers_do_not_block_profile_updates(pool: PgPool) {
        sqlx::query(
            "INSERT INTO auth_users (username, email, first_name, last_name, password, role_id) \
             VALUES ('Legacy', 'legacy@example.org', 'Old', 'User', 'hash', 3), \
                    ('legacy', 'other@example.org', 'Other', 'User', 'hash', 3)",
        )
        .execute(&pool)
        .await
        .expect("legacy duplicates should be represented in the test database");

        let id: i32 = sqlx::query_scalar("SELECT id FROM auth_users WHERE username = 'Legacy'")
            .fetch_one(&pool)
            .await
            .expect("legacy user should exist");
        let update = AuthUser {
            username: "Legacy".into(),
            email: "legacy@example.org".into(),
            first_name: "Updated".into(),
            ..Default::default()
        };

        update_auth_user(&pool, id, &update)
            .await
            .expect("updating profile fields should remain possible");
        assert!(matches!(
            insert_user(&pool, "LEGACY", "new@example.org").await,
            Err(NurError::Conflict(_))
        ));
    }
}
