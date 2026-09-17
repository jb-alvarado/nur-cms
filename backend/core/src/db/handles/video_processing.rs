use sqlx::{FromRow, Postgres, Transaction, postgres::PgPool};
use uuid::Uuid;

use crate::VIDEO_PROCESSING_MAX_ATTEMPTS;

pub const VIDEO_VARIANTS_JOB_KIND: &str = "video_variants";

#[derive(Debug, FromRow)]
pub struct VideoProcessingJob {
    pub id: i64,
    pub attempts: i32,
    pub max_attempts: i32,
    pub lease_token: String,
    pub media_id: i32,
    pub filename: String,
    pub path: String,
    pub mime_type: Option<String>,
    pub kind: String,
    pub source_media_id: Option<i32>,
}

pub struct ClaimedVideoProcessingJob {
    pub job: VideoProcessingJob,
    pub queued_jobs: i64,
}

pub struct QueuedVideo {
    pub id: i32,
    pub path: String,
    pub filename: String,
}

pub struct MediaSource {
    pub filename: String,
    pub path: String,
    pub mime_type: Option<String>,
}

pub struct VideoVariantRecord {
    pub profile: String,
    pub width: i32,
    pub height: i32,
    pub container: String,
    pub video_codec: String,
    pub audio_codec: Option<String>,
    pub filename: String,
    pub size: i64,
    pub duration_ms: Option<i64>,
}

pub struct ThumbnailRecord {
    pub width: i32,
    pub height: i32,
    pub filename: String,
}

pub struct ReplacedOutputFiles {
    pub video_variants: Vec<String>,
    pub thumbnails: Vec<String>,
}

pub async fn ensure_video_processing_job(pool: &PgPool, media_id: i32) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let inserted = sqlx::query(
        r#"INSERT INTO media_processing_jobs (media_id, kind, max_attempts)
           SELECT $1, $2, $3
           WHERE NOT EXISTS (
               SELECT 1 FROM media_processing_jobs
               WHERE media_id = $1 AND kind = $2
           )
           ON CONFLICT (media_id) WHERE status IN ('queued', 'running') DO NOTHING"#,
    )
    .bind(media_id)
    .bind(VIDEO_VARIANTS_JOB_KIND)
    .bind(*VIDEO_PROCESSING_MAX_ATTEMPTS)
    .execute(&mut *transaction)
    .await?;

    if inserted.rows_affected() == 1 {
        set_media_processing_status_on(&mut transaction, media_id, "queued").await?;
    }

    transaction.commit().await
}

pub async fn enqueue_video_processing_job(
    pool: &PgPool,
    media_id: i32,
    kind: &str,
    source_media_id: Option<i32>,
    max_attempts: i32,
    update_media_status: bool,
) -> Result<bool, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let inserted = sqlx::query(
        r#"INSERT INTO media_processing_jobs (media_id, kind, source_media_id, max_attempts)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (media_id) WHERE status IN ('queued', 'running') DO NOTHING"#,
    )
    .bind(media_id)
    .bind(kind)
    .bind(source_media_id)
    .bind(max_attempts)
    .execute(&mut *transaction)
    .await?;

    if inserted.rows_affected() == 0 {
        return Ok(false);
    }

    if update_media_status {
        set_media_processing_status_on(&mut transaction, media_id, "queued").await?;
    }

    transaction.commit().await?;

    Ok(true)
}

pub async fn set_media_processing_status(
    pool: &PgPool,
    media_id: i32,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE media SET processing_status = $1 WHERE id = $2")
        .bind(status)
        .bind(media_id)
        .execute(pool)
        .await?;

    Ok(())
}

async fn set_media_processing_status_on(
    transaction: &mut Transaction<'_, Postgres>,
    media_id: i32,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE media SET processing_status = $1 WHERE id = $2")
        .bind(status)
        .bind(media_id)
        .execute(&mut **transaction)
        .await?;

    Ok(())
}

pub async fn queued_videos_without_jobs(pool: &PgPool) -> Result<Vec<QueuedVideo>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (i32, String, String)>(
        r#"SELECT id, path, filename
           FROM media
           WHERE type LIKE 'video/%' AND processing_status = 'queued'
             AND NOT EXISTS (
                 SELECT 1 FROM media_processing_jobs jobs
                 WHERE jobs.media_id = media.id AND jobs.status IN ('queued', 'running')
             )"#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, path, filename)| QueuedVideo { id, path, filename })
        .collect())
}

pub async fn video_processing_job_is_active(
    pool: &PgPool,
    job_id: i64,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM media_processing_jobs WHERE id = $1 AND status IN ('queued', 'running'))",
    )
    .bind(job_id)
    .fetch_one(pool)
    .await
}

pub async fn claim_video_processing_job(
    pool: &PgPool,
    main_job_kind: &str,
    lease_seconds: i64,
    retry_base_delay_seconds: i64,
) -> Result<Option<ClaimedVideoProcessingJob>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let lease_token = Uuid::new_v4().to_string();

    sqlx::query(
        r#"UPDATE media_processing_jobs
           SET status = 'queued', locked_at = NULL, lease_expires_at = NULL,
               lease_token = NULL
           WHERE status = 'running' AND lease_expires_at < now()"#,
    )
    .execute(&mut *transaction)
    .await?;

    sqlx::query(
        r#"WITH failed_jobs AS (
                UPDATE media_processing_jobs
                SET status = 'failed', finished_at = now(), updated_at = now()
                WHERE status = 'queued' AND attempts >= max_attempts
                RETURNING media_id, kind
            )
            UPDATE media
            SET processing_status = CASE
                WHEN failed_jobs.kind = $1 THEN 'failed'
                ELSE 'completed'
            END
            FROM failed_jobs
            WHERE media.id = failed_jobs.media_id"#,
    )
    .bind(main_job_kind)
    .execute(&mut *transaction)
    .await?;

    let job_id = sqlx::query_scalar::<_, i64>(
        r#"WITH next_job AS (
                SELECT id
                FROM media_processing_jobs
                WHERE status = 'queued' AND attempts < max_attempts
                  AND (
                      attempts = 0 OR
                      updated_at <= now() - (
                          LEAST(900, $3::BIGINT * (1::BIGINT << LEAST(attempts - 1, 5)))
                          * interval '1 second'
                      )
                  )
                ORDER BY created_at, id
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            UPDATE media_processing_jobs jobs
            SET status = 'running',
                attempts = attempts + 1,
                locked_at = now(),
                lease_expires_at = now() + ($1::BIGINT * interval '1 second'),
                lease_token = $2,
                started_at = COALESCE(started_at, now()),
                updated_at = now()
            FROM next_job
            WHERE jobs.id = next_job.id
            RETURNING jobs.id"#,
    )
    .bind(lease_seconds)
    .bind(&lease_token)
    .bind(retry_base_delay_seconds)
    .fetch_optional(&mut *transaction)
    .await?;

    let Some(job_id) = job_id else {
        transaction.commit().await?;
        return Ok(None);
    };

    let job = sqlx::query_as::<_, VideoProcessingJob>(
        r#"SELECT jobs.id, jobs.attempts, jobs.max_attempts, jobs.lease_token, jobs.media_id, media.filename, media.path,
                  media.type AS mime_type, jobs.kind, jobs.source_media_id
           FROM media_processing_jobs jobs
           JOIN media ON media.id = jobs.media_id
           WHERE jobs.id = $1"#,
    )
    .bind(job_id)
    .fetch_optional(&mut *transaction)
    .await?;

    let Some(job) = job else {
        transaction.commit().await?;
        return Ok(None);
    };

    if job.kind == main_job_kind {
        set_media_processing_status_on(&mut transaction, job.media_id, "processing").await?;
    }

    let queued_jobs = sqlx::query_scalar(
        "SELECT count(*) FROM media_processing_jobs WHERE status = 'queued' AND attempts < max_attempts",
    )
    .fetch_one(&mut *transaction)
    .await?;

    transaction.commit().await?;

    Ok(Some(ClaimedVideoProcessingJob { job, queued_jobs }))
}

pub async fn release_cancelled_video_processing_job(
    pool: &PgPool,
    job: &VideoProcessingJob,
    update_media_status: bool,
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let result = sqlx::query(
        "UPDATE media_processing_jobs SET status = 'queued', attempts = GREATEST(attempts - 1, 0), locked_at = NULL, lease_expires_at = NULL, lease_token = NULL, updated_at = now() WHERE id = $1 AND lease_token = $2 AND status = 'running'",
    )
    .bind(job.id)
    .bind(&job.lease_token)
    .execute(&mut *transaction)
    .await?;

    if result.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }

    if update_media_status {
        set_media_processing_status_on(&mut transaction, job.media_id, "queued").await?;
    }

    transaction.commit().await
}

pub async fn renew_video_processing_lease(
    pool: &PgPool,
    job_id: i64,
    lease_token: &str,
    lease_seconds: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE media_processing_jobs SET lease_expires_at = now() + ($1::BIGINT * interval '1 second'), updated_at = now() WHERE id = $2 AND lease_token = $3 AND status = 'running'",
    )
    .bind(lease_seconds)
    .bind(job_id)
    .bind(lease_token)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() == 1)
}

pub async fn fail_video_processing_job(
    pool: &PgPool,
    job: &VideoProcessingJob,
    reason: &str,
    retryable: bool,
    retry_media_status: &str,
    terminal_media_status: &str,
) -> Result<bool, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let attempts_exhausted = sqlx::query_scalar::<_, bool>(
        "SELECT attempts >= max_attempts FROM media_processing_jobs WHERE id = $1 AND lease_token = $2 AND status = 'running' FOR UPDATE",
    )
    .bind(job.id)
    .bind(&job.lease_token)
    .fetch_one(&mut *transaction)
    .await?;
    let will_retry = retryable && !attempts_exhausted;
    let status = if will_retry { "queued" } else { "failed" };

    let updated = sqlx::query(
        "UPDATE media_processing_jobs SET status = $1, locked_at = NULL, lease_expires_at = NULL, lease_token = NULL, last_error = $2, finished_at = CASE WHEN $1 = 'failed' THEN now() ELSE NULL END, updated_at = now() WHERE id = $3 AND lease_token = $4 AND status = 'running'",
    )
    .bind(status)
    .bind(reason)
    .bind(job.id)
    .bind(&job.lease_token)
    .execute(&mut *transaction)
    .await?;

    if updated.rows_affected() != 1 {
        return Err(sqlx::Error::RowNotFound);
    }

    let media_status = if will_retry {
        retry_media_status
    } else {
        terminal_media_status
    };
    set_media_processing_status_on(&mut transaction, job.media_id, media_status).await?;

    transaction.commit().await?;

    Ok(will_retry)
}

pub async fn select_media_source(
    pool: &PgPool,
    media_id: i32,
) -> Result<Option<MediaSource>, sqlx::Error> {
    let row = sqlx::query_as("SELECT filename, path, type FROM media WHERE id = $1")
        .bind(media_id)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|(filename, path, mime_type)| MediaSource {
        filename,
        path,
        mime_type,
    }))
}

pub async fn lock_owned_video_processing_job<'a>(
    pool: &'a PgPool,
    job: &VideoProcessingJob,
) -> Result<Option<Transaction<'a, Postgres>>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let owned = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM media_processing_jobs WHERE id = $1 AND lease_token = $2 AND status = 'running' FOR UPDATE",
    )
    .bind(job.id)
    .bind(&job.lease_token)
    .fetch_optional(&mut *transaction)
    .await?;

    Ok(owned.map(|_| transaction))
}

pub async fn output_filename_conflicts(
    transaction: &mut Transaction<'_, Postgres>,
    public_path: &str,
    filename: &str,
    media_id: i32,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT EXISTS (
            SELECT 1 FROM media m
            WHERE m.path = $1 AND m.filename = $2 AND m.id <> $3
            UNION ALL
            SELECT 1 FROM media_variants mv
            JOIN media owner ON owner.id = mv.media_id
            WHERE owner.path = $1 AND mv.filename = $2 AND owner.id <> $3
            UNION ALL
            SELECT 1 FROM media_video_variants vv
            JOIN media owner ON owner.id = vv.media_id
            WHERE owner.path = $1 AND vv.filename = $2 AND vv.media_id <> $3
        )"#,
    )
    .bind(public_path)
    .bind(filename)
    .bind(media_id)
    .fetch_one(&mut **transaction)
    .await
}

pub async fn persist_video_outputs(
    mut transaction: Transaction<'_, Postgres>,
    job: &VideoProcessingJob,
    variants: &[VideoVariantRecord],
    thumbnails: &[ThumbnailRecord],
) -> Result<ReplacedOutputFiles, sqlx::Error> {
    let old_variant_filenames =
        sqlx::query_scalar("SELECT filename FROM media_video_variants WHERE media_id = $1")
            .bind(job.media_id)
            .fetch_all(&mut *transaction)
            .await?;

    for variant in variants {
        sqlx::query(
            r#"INSERT INTO media_video_variants
                   (media_id, kind, profile, width, height, container, video_codec,
                    audio_codec, filename, size, duration_ms)
               VALUES ($1, 'progressive', $2, $3, $4, $5, $6, $7, $8, $9, $10)
               ON CONFLICT (media_id, kind, profile) DO UPDATE SET
                   width = EXCLUDED.width, height = EXCLUDED.height,
                   container = EXCLUDED.container, video_codec = EXCLUDED.video_codec,
                   audio_codec = EXCLUDED.audio_codec, filename = EXCLUDED.filename,
                   size = EXCLUDED.size, duration_ms = EXCLUDED.duration_ms"#,
        )
        .bind(job.media_id)
        .bind(&variant.profile)
        .bind(variant.width)
        .bind(variant.height)
        .bind(&variant.container)
        .bind(&variant.video_codec)
        .bind(&variant.audio_codec)
        .bind(&variant.filename)
        .bind(variant.size)
        .bind(variant.duration_ms)
        .execute(&mut *transaction)
        .await?;
    }

    let current_profiles: Vec<&str> = variants
        .iter()
        .map(|variant| variant.profile.as_str())
        .collect();
    sqlx::query("DELETE FROM media_video_variants WHERE media_id = $1 AND NOT (profile = ANY($2))")
        .bind(job.media_id)
        .bind(&current_profiles)
        .execute(&mut *transaction)
        .await?;

    let old_thumbnail_filenames =
        replace_thumbnails(&mut transaction, job.media_id, thumbnails, true).await?;
    complete_job(&mut transaction, job).await?;
    transaction.commit().await?;

    Ok(ReplacedOutputFiles {
        video_variants: old_variant_filenames,
        thumbnails: old_thumbnail_filenames,
    })
}

pub async fn persist_thumbnail_outputs(
    mut transaction: Transaction<'_, Postgres>,
    job: &VideoProcessingJob,
    thumbnails: &[ThumbnailRecord],
) -> Result<Vec<String>, sqlx::Error> {
    let old_filenames =
        replace_thumbnails(&mut transaction, job.media_id, thumbnails, false).await?;

    complete_job(&mut transaction, job).await?;
    transaction.commit().await?;

    Ok(old_filenames)
}

async fn replace_thumbnails(
    transaction: &mut Transaction<'_, Postgres>,
    media_id: i32,
    thumbnails: &[ThumbnailRecord],
    ignore_conflicts: bool,
) -> Result<Vec<String>, sqlx::Error> {
    let old_filenames =
        sqlx::query_scalar("SELECT filename FROM media_variants WHERE media_id = $1")
            .bind(media_id)
            .fetch_all(&mut **transaction)
            .await?;

    sqlx::query("DELETE FROM media_variants WHERE media_id = $1")
        .bind(media_id)
        .execute(&mut **transaction)
        .await?;

    for thumbnail in thumbnails {
        if ignore_conflicts {
            sqlx::query(
                r#"INSERT INTO media_variants (media_id, width, height, filename)
                   VALUES ($1, $2, $3, $4)
                   ON CONFLICT (media_id, width, height, filename) DO NOTHING"#,
            )
            .bind(media_id)
            .bind(thumbnail.width)
            .bind(thumbnail.height)
            .bind(&thumbnail.filename)
            .execute(&mut **transaction)
            .await?;
        } else {
            sqlx::query(
                r#"INSERT INTO media_variants (media_id, width, height, filename)
                   VALUES ($1, $2, $3, $4)"#,
            )
            .bind(media_id)
            .bind(thumbnail.width)
            .bind(thumbnail.height)
            .bind(&thumbnail.filename)
            .execute(&mut **transaction)
            .await?;
        }
    }

    Ok(old_filenames)
}

async fn complete_job(
    transaction: &mut Transaction<'_, Postgres>,
    job: &VideoProcessingJob,
) -> Result<(), sqlx::Error> {
    let completed = sqlx::query(
        "UPDATE media_processing_jobs SET status = 'completed', finished_at = now(), locked_at = NULL, lease_expires_at = NULL, lease_token = NULL, last_error = NULL, updated_at = now() WHERE id = $1 AND lease_token = $2 AND status = 'running'",
    )
    .bind(job.id)
    .bind(&job.lease_token)
    .execute(&mut **transaction)
    .await?;

    if completed.rows_affected() != 1 {
        return Err(sqlx::Error::RowNotFound);
    }

    set_media_processing_status_on(transaction, job.media_id, "completed").await
}
