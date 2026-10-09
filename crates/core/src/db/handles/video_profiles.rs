use sqlx::{Postgres, Transaction, postgres::PgPool};

use crate::{
    db::models::{VideoProfile, VideoSettings},
    file::video::{hls_profiles, validate_video_profile},
    utils::errors::NurError,
};

pub async fn select_video_settings(pool: &PgPool) -> Result<VideoSettings, sqlx::Error> {
    let delivery_mode = sqlx::query_scalar("SELECT delivery_mode FROM video_settings WHERE id = 1")
        .fetch_one(pool)
        .await?;
    Ok(VideoSettings { delivery_mode })
}

pub async fn update_video_settings(
    pool: &PgPool,
    settings: &VideoSettings,
) -> Result<(), NurError> {
    let mut transaction = pool.begin().await?;
    lock_video_profiles(&mut transaction).await?;
    if settings.delivery_mode == "hls" {
        validate_hls_profiles(&mut transaction).await?;
    }
    sqlx::query("UPDATE video_settings SET delivery_mode = $1 WHERE id = 1")
        .bind(&settings.delivery_mode)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

async fn lock_video_profiles(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<(), sqlx::Error> {
    sqlx::query("LOCK TABLE video_profiles IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn validate_hls_profiles(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<(), NurError> {
    let profiles = sqlx::query_as::<_, VideoProfile>(
        r#"SELECT id, name, container, height, cmd, enabled, hls_enabled, sort_order,
                  NULL::BIGINT AS total_count
           FROM video_profiles WHERE hls_enabled = true ORDER BY sort_order, id"#,
    )
    .fetch_all(&mut **transaction)
    .await?;
    for profile in &profiles {
        validate_video_profile(profile).map_err(NurError::UnprocessableEntity)?;
    }
    hls_profiles(&profiles).map_err(NurError::UnprocessableEntity)?;
    Ok(())
}

async fn validate_hls_profiles_if_needed(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<(), NurError> {
    let needed: bool = sqlx::query_scalar(
        r#"SELECT (SELECT delivery_mode = 'hls' FROM video_settings WHERE id = 1)
                  OR EXISTS (SELECT 1 FROM media WHERE video_delivery_mode = 'hls')"#,
    )
    .fetch_one(&mut **transaction)
    .await?;
    if needed {
        validate_hls_profiles(transaction).await?;
    }
    Ok(())
}

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
    let mut transaction = pool.begin().await?;
    lock_video_profiles(&mut transaction).await?;

    let id = sqlx::query_scalar(
        r#"INSERT INTO video_profiles (name, container, height, cmd, enabled, hls_enabled, sort_order)
           VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id"#,
    )
    .bind(&profile.name)
    .bind(&profile.container)
    .bind(profile.height)
    .bind(cmd)
    .bind(profile.enabled)
    .bind(profile.hls_enabled)
    .bind(profile.sort_order)
    .fetch_one(&mut *transaction)
    .await
    .map_err(video_profile_write_error)?;
    validate_hls_profiles_if_needed(&mut transaction).await?;
    transaction.commit().await?;
    Ok(id)
}

/// Updates a video profile while binding its `cmd` as one JSONB document.
pub async fn update_video_profile(
    pool: &PgPool,
    id: i32,
    profile: &VideoProfile,
) -> Result<(), NurError> {
    let cmd = serde_json::to_value(&profile.cmd)?;
    let mut transaction = pool.begin().await?;
    lock_video_profiles(&mut transaction).await?;

    let result = sqlx::query(
        r#"UPDATE video_profiles
           SET name = $1, container = $2, height = $3, cmd = $4, enabled = $5, hls_enabled = $6, sort_order = $7
           WHERE id = $8"#,
    )
    .bind(&profile.name)
    .bind(&profile.container)
    .bind(profile.height)
    .bind(cmd)
    .bind(profile.enabled)
    .bind(profile.hls_enabled)
    .bind(profile.sort_order)
    .bind(id)
    .execute(&mut *transaction)
    .await
    .map_err(video_profile_write_error)?;

    if result.rows_affected() == 0 {
        return Err(NurError::NotFound);
    }

    ensure_enabled_profile(&mut transaction).await?;
    validate_hls_profiles_if_needed(&mut transaction).await?;
    transaction.commit().await?;

    Ok(())
}

pub async fn delete_video_profile(pool: &PgPool, id: i32) -> Result<(), NurError> {
    let mut transaction = pool.begin().await?;
    lock_video_profiles(&mut transaction).await?;
    let result = sqlx::query("DELETE FROM video_profiles WHERE id = $1")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    if result.rows_affected() == 0 {
        return Err(NurError::NotFound);
    }
    ensure_enabled_profile(&mut transaction).await?;
    validate_hls_profiles_if_needed(&mut transaction).await?;
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
pub async fn enabled_video_profiles(
    pool: &PgPool,
    mode: &str,
) -> Result<Vec<VideoProfile>, sqlx::Error> {
    sqlx::query_as::<_, VideoProfile>(
        r#"SELECT id, name, container, height, cmd, enabled, hls_enabled, sort_order, NULL::BIGINT AS total_count
           FROM video_profiles
           WHERE (CASE WHEN $1 = 'hls' THEN hls_enabled ELSE enabled END) = true
           ORDER BY sort_order, id"#,
    )
    .bind(mode)
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use crate::db::models::{VideoProfile, VideoProfileArg, VideoSettings};
    use crate::db::{
        fields::{Table, VideoProfileFields},
        queries::QueryObj,
    };

    use super::{
        delete_video_profile, enabled_video_profiles, insert_video_profile, update_video_profile,
        update_video_settings,
    };

    const MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

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
            hls_enabled: false,
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

        let profiles = enabled_video_profiles(&pool, "file")
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

        let profiles = enabled_video_profiles(&pool, "file")
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

        let profiles = enabled_video_profiles(&pool, "file")
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
    async fn active_hls_mode_keeps_a_compatible_h264_profile(pool: PgPool) {
        insert_video_profile(&pool, &sample_profile("file-only"))
            .await
            .expect("a file-only profile can be added");
        update_video_settings(
            &pool,
            &VideoSettings {
                delivery_mode: "hls".into(),
            },
        )
        .await
        .expect("default HLS profiles are valid");

        let h264_ids: Vec<i32> = sqlx::query_scalar(
            "SELECT id FROM video_profiles WHERE name IN ('h264-480', 'h264-720', 'h264-1080') ORDER BY height",
        )
        .fetch_all(&pool)
        .await
        .expect("H.264 profiles can be read");
        for id in &h264_ids[..2] {
            delete_video_profile(&pool, *id)
                .await
                .expect("other H.264 profiles remain");
        }
        assert!(matches!(
            delete_video_profile(&pool, h264_ids[2]).await,
            Err(crate::utils::errors::NurError::UnprocessableEntity(_))
        ));

        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM video_profiles WHERE id = $1")
                .bind(h264_ids[2])
                .fetch_one(&pool)
                .await
                .expect("remaining profile can be counted");
        assert_eq!(remaining, 1);
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
