use sqlx::postgres::PgPool;

use crate::{db::models::VideoProfile, utils::errors::NurError};

fn video_profile_write_error(error: sqlx::Error) -> NurError {
    if error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "23505")
    {
        NurError::Conflict("A video profile with this name already exists".into())
    } else {
        error.into()
    }
}

/// Inserts a video profile while binding its `cmd` as one JSONB document.
///
/// `cmd` is represented as a Rust vector, but PostgreSQL stores it in a
/// `jsonb` column. Binding the serialized JSON value explicitly prevents SQLx
/// from interpreting the vector as a PostgreSQL `jsonb[]` array.
pub async fn insert_video_profile(pool: &PgPool, profile: &VideoProfile) -> Result<i32, NurError> {
    let cmd = serde_json::to_value(&profile.cmd)?;

    sqlx::query_scalar(
        r#"INSERT INTO video_profiles (name, container, height, cmd, enabled, sort_order)
           VALUES ($1, $2, $3, $4, $5, $6) RETURNING id"#,
    )
    .bind(&profile.name)
    .bind(&profile.container)
    .bind(profile.height)
    .bind(cmd)
    .bind(profile.enabled)
    .bind(profile.sort_order)
    .fetch_one(pool)
    .await
    .map_err(video_profile_write_error)
}

/// Updates a video profile while binding its `cmd` as one JSONB document.
pub async fn update_video_profile(
    pool: &PgPool,
    id: i32,
    profile: &VideoProfile,
) -> Result<(), NurError> {
    let cmd = serde_json::to_value(&profile.cmd)?;
    let mut transaction = pool.begin().await?;
    sqlx::query("LOCK TABLE video_profiles IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *transaction)
        .await?;

    let result = sqlx::query(
        r#"UPDATE video_profiles
           SET name = $1, container = $2, height = $3, cmd = $4, enabled = $5, sort_order = $6
           WHERE id = $7"#,
    )
    .bind(&profile.name)
    .bind(&profile.container)
    .bind(profile.height)
    .bind(cmd)
    .bind(profile.enabled)
    .bind(profile.sort_order)
    .bind(id)
    .execute(&mut *transaction)
    .await
    .map_err(video_profile_write_error)?;

    if result.rows_affected() == 0 {
        return Err(NurError::NotFound);
    }

    ensure_enabled_profile(&mut transaction).await?;
    transaction.commit().await?;

    Ok(())
}

pub async fn delete_video_profile(pool: &PgPool, id: i32) -> Result<(), NurError> {
    let mut transaction = pool.begin().await?;
    sqlx::query("LOCK TABLE video_profiles IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *transaction)
        .await?;
    let result = sqlx::query("DELETE FROM video_profiles WHERE id = $1")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    if result.rows_affected() == 0 {
        return Err(NurError::NotFound);
    }
    ensure_enabled_profile(&mut transaction).await?;
    transaction.commit().await?;
    Ok(())
}

async fn ensure_enabled_profile(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<(), NurError> {
    let enabled = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM video_profiles WHERE enabled = true)",
    )
    .fetch_one(&mut **transaction)
    .await?;
    if enabled {
        Ok(())
    } else {
        Err(NurError::Conflict(
            "At least one video profile must remain enabled.".into(),
        ))
    }
}

/// Returns the enabled video profiles ordered for processing, used by the
/// video transcoding pipeline instead of the removed `NUR_VIDEO_PROFILES` env var.
pub async fn enabled_video_profiles(pool: &PgPool) -> Result<Vec<VideoProfile>, sqlx::Error> {
    sqlx::query_as::<_, VideoProfile>(
        r#"SELECT id, name, container, height, cmd, enabled, sort_order, NULL::BIGINT AS total_count
           FROM video_profiles
           WHERE enabled = true
           ORDER BY sort_order, id"#,
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use crate::db::models::{VideoProfile, VideoProfileArg};
    use crate::db::{
        fields::{Table, VideoProfileFields},
        queries::QueryObj,
    };

    use super::{
        delete_video_profile, enabled_video_profiles, insert_video_profile, update_video_profile,
    };

    const MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

    fn sample_profile(name: &str) -> VideoProfile {
        VideoProfile {
            id: 0,
            name: name.into(),
            container: "mp4".into(),
            height: 480,
            cmd: vec![VideoProfileArg {
                flag: "-c:v".into(),
                value: "libx264".into(),
            }],
            enabled: true,
            sort_order: 0,
            total_count: None,
        }
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn inserts_and_reads_back_a_video_profile(pool: PgPool) {
        let profile = sample_profile("custom-480");
        let id = insert_video_profile(&pool, &profile)
            .await
            .expect("insert should succeed");

        let profiles = enabled_video_profiles(&pool)
            .await
            .expect("select should succeed");
        let inserted = profiles
            .iter()
            .find(|candidate| candidate.id == id)
            .expect("the inserted profile should be present");
        assert_eq!(inserted.cmd, profile.cmd);
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn rejects_a_duplicate_name(pool: PgPool) {
        let profile = sample_profile("duplicate");
        insert_video_profile(&pool, &profile)
            .await
            .expect("first insert should succeed");

        let error = insert_video_profile(&pool, &profile)
            .await
            .expect_err("second insert with the same name should fail");
        assert!(matches!(error, crate::utils::errors::NurError::Conflict(_)));
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn excludes_disabled_profiles_from_the_processing_list(pool: PgPool) {
        let mut profile = sample_profile("disabled");
        profile.enabled = false;
        insert_video_profile(&pool, &profile)
            .await
            .expect("insert should succeed");

        let profiles = enabled_video_profiles(&pool)
            .await
            .expect("select should succeed");
        assert!(
            !profiles
                .iter()
                .any(|candidate| candidate.name == "disabled")
        );
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn updates_an_existing_video_profile(pool: PgPool) {
        let profile = sample_profile("updatable");
        let id = insert_video_profile(&pool, &profile)
            .await
            .expect("insert should succeed");

        let mut updated = profile.clone();
        updated.height = 720;
        update_video_profile(&pool, id, &updated)
            .await
            .expect("update should succeed");

        let profiles = enabled_video_profiles(&pool)
            .await
            .expect("select should succeed");
        let updated_profile = profiles
            .iter()
            .find(|candidate| candidate.id == id)
            .expect("the updated profile should be present");
        assert_eq!(updated_profile.height, 720);
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn refuses_to_disable_or_delete_the_last_enabled_profile(pool: PgPool) {
        sqlx::query("DELETE FROM video_profiles")
            .execute(&pool)
            .await
            .expect("default profiles can be removed for the test");
        let profile = sample_profile("last-enabled");
        let id = insert_video_profile(&pool, &profile)
            .await
            .expect("profile can be inserted");

        let mut disabled = profile.clone();
        disabled.enabled = false;
        assert!(matches!(
            update_video_profile(&pool, id, &disabled).await,
            Err(crate::utils::errors::NurError::Conflict(_))
        ));
        assert!(matches!(
            delete_video_profile(&pool, id).await,
            Err(crate::utils::errors::NurError::Conflict(_))
        ));
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn supports_selecting_only_requested_profile_fields(pool: PgPool) {
        let response = crate::db::handles::select_record::<VideoProfileFields, VideoProfile>(
            &pool,
            &Table::VideoProfiles,
            QueryObj {
                fields: vec![VideoProfileFields::Name],
                ..Default::default()
            },
        )
        .await
        .expect("partial profile selection should not fail");
        assert!(!response.results.is_empty());
        assert!(!response.results[0].name.is_empty());
    }
}
