use rand::{Rng, distr::Alphanumeric};
use sqlx::postgres::PgPool;

#[cfg(debug_assertions)]
use {
    std::env,
    tokio::fs,
    tracing::{debug, info},
};

#[cfg(debug_assertions)]
use crate::db::{
    fields::MediaFields,
    format_sql,
    handles::{insert_record, select_auth_user, select_record},
    models::{AuthUser, Media},
};

use crate::db::{
    fields::TSLanguage,
    models::{Configuration, MailTarget, TSConfig},
    queries::{QueryObj, RespondObj},
};
use crate::utils::errors::NurError;

pub async fn db_migrate(pool: &PgPool) -> Result<(), NurError> {
    sqlx::migrate!("../../migrations").run(pool).await?;

    if select_configuration(pool).await.is_err() {
        let secret: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(80)
            .map(char::from)
            .collect();

        const QUERY: &str = "INSERT INTO configuration(jwt_secret, image_extensions, image_resolutions) VALUES($1, ARRAY['jpg', 'avif', 'webp'], ARRAY[1024, 480]);";

        sqlx::query(QUERY).bind(secret).execute(pool).await?;
    }

    Ok(())
}

#[cfg(debug_assertions)]
pub async fn dev_migrate(pool: &PgPool) -> Result<(), NurError> {
    let query: QueryObj<MediaFields> = QueryObj {
        limit: 1,
        fields: vec![MediaFields::ID],
        ..Default::default()
    };

    let auth_resp = select_auth_user(pool, QueryObj::default()).await?;
    let media_resp =
        select_record::<MediaFields, Media>(pool, &crate::db::fields::Table::Media, query).await?;

    if auth_resp.results.is_empty() {
        let user = AuthUser::new(
            "admin@example.org".to_string(),
            "admin".to_string(),
            "Ad".to_string(),
            "Min".to_string(),
            "admin".to_string(),
            1,
        );

        insert_record::<AuthUser, i32>(pool, &crate::db::fields::Table::AuthUsers, &user).await?;

        if media_resp.results.is_empty() {
            let mut migrations_path = env::current_dir()?.join("migrations_dev");

            if !migrations_path.is_dir() {
                migrations_path = env::current_dir()?
                    .join("../migrations_dev")
                    .canonicalize()?;
            }

            let mut rd = fs::read_dir(migrations_path).await?;
            let mut migrations = Vec::new();
            while let Some(entry) = rd.next_entry().await? {
                if entry
                    .path()
                    .extension()
                    .map(|ext| ext == "sql")
                    .unwrap_or(false)
                {
                    migrations.push(entry);
                }
            }

            migrations.sort_by_key(fs::DirEntry::path);

            for entry in migrations {
                use sqlx::Executor;

                let path = entry.path();
                let sql = fs::read_to_string(&path).await?;
                info!("Executing dev migration: {:?}", path.file_name().unwrap());

                pool.execute(&*sql).await?;
            }
        }
    }

    Ok(())
}

pub async fn select_configuration(pool: &PgPool) -> Result<Configuration, NurError> {
    const QUERY: &str = "select * from configuration;";

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(QUERY));

    let data: Configuration = sqlx::query_as(QUERY).fetch_one(pool).await?;

    Ok(data)
}

pub async fn select_mail_target(pool: &PgPool, name: &str) -> Result<MailTarget, NurError> {
    const QUERY: &str =
        "select id, name, subject, recipients, allow_html FROM mail_targets WHERE name = $1;";

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(QUERY));

    let data: MailTarget = sqlx::query_as(QUERY).bind(name).fetch_one(pool).await?;

    Ok(data)
}

pub async fn select_mail_targets(pool: &PgPool) -> Result<RespondObj<MailTarget>, NurError> {
    const QUERY: &str = "select id, name, subject, recipients, allow_html FROM mail_targets;";

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(QUERY));

    let data: Vec<MailTarget> = sqlx::query_as(QUERY).fetch_all(pool).await?;
    let resp = RespondObj {
        count: data.first().and_then(|d| d.total_count).unwrap_or_default(),
        previous: None,
        next: None,
        results: data,
    };

    Ok(resp)
}

pub async fn select_ts_language(pool: &PgPool) -> Result<RespondObj<TSConfig>, NurError> {
    const QUERY: &str =
        "select cfgname, count(*) OVER() AS total_count from pg_catalog.pg_ts_config;";
    let query_obj: QueryObj<TSLanguage> = QueryObj {
        limit: 200,
        ..Default::default()
    };

    #[cfg(debug_assertions)]
    debug!("{}", format_sql(QUERY));

    let data: Vec<TSConfig> = sqlx::query_as(QUERY).fetch_all(pool).await?;

    Ok(RespondObj::new(&query_obj, data))
}
