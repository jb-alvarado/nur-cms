use std::{
    collections::HashSet,
    env,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use serde::Deserialize;
use sqlx::{FromRow, Postgres, Transaction, postgres::PgPool};
use tokio::{fs, process::Command, sync::broadcast::Sender, time};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    CONFIG, STORAGE, VIDEO_PROCESSING_CONCURRENCY, VIDEO_PROCESSING_LEASE_SECONDS,
    VIDEO_PROCESSING_MAX_ATTEMPTS, VIDEO_PROCESSING_THREADS, VIDEO_PROCESSING_TIMEOUT_SECONDS,
    db::{
        handles,
        models::{VideoProfile, VideoProfileArg},
    },
    file::{helper::contained_storage_target, processing::save_image},
    sse::{SSELevel as Level, SSEMessage},
    utils::errors::NurError,
};

const JOB_KIND: &str = "video_variants";
const WORKER_IDLE_DELAY: Duration = Duration::from_secs(2);
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, FromRow)]
struct VideoJob {
    id: i64,
    lease_token: String,
    media_id: i32,
    filename: String,
    path: String,
    mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeResult {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

#[derive(Debug)]
struct VideoInfo {
    width: u32,
    height: u32,
    duration_ms: Option<i64>,
}

#[derive(Debug)]
struct ProcessedVariant {
    profile: VideoProfile,
    filename: String,
    staging_path: PathBuf,
    width: i32,
    height: i32,
    size: i64,
    duration_ms: Option<i64>,
}

#[derive(Debug)]
struct ProcessedThumbnail {
    filename: String,
    staging_path: PathBuf,
    width: i32,
    height: i32,
}

pub async fn enqueue_video_processing(pool: &PgPool, media_id: i32) -> Result<(), NurError> {
    let mut transaction = pool.begin().await?;
    let inserted = sqlx::query(
        r#"INSERT INTO media_processing_jobs (media_id, kind, max_attempts)
           VALUES ($1, $2, $3)
           ON CONFLICT (media_id, kind) WHERE status IN ('queued', 'running') DO NOTHING"#,
    )
    .bind(media_id)
    .bind(JOB_KIND)
    .bind(*VIDEO_PROCESSING_MAX_ATTEMPTS)
    .execute(&mut *transaction)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(NurError::Conflict(
            "Video processing is already queued or running.".into(),
        ));
    }
    sqlx::query("UPDATE media SET processing_status = 'queued' WHERE id = $1")
        .bind(media_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn mark_video_processing_failed(pool: &PgPool, media_id: i32) {
    if let Err(error) = sqlx::query("UPDATE media SET processing_status = 'failed' WHERE id = $1")
        .bind(media_id)
        .execute(pool)
        .await
    {
        error!(media_id, %error, "Failed to mark video processing as failed");
    }
}

/// Starts independently leased workers. A crashed process leaves its jobs to be
/// reclaimed by a later process after the lease expires.
pub fn start_video_workers(pool: PgPool, tx: Sender<String>) {
    let cleanup_pool = pool.clone();
    tokio::spawn(async move {
        if let Err(error) = cleanup_inactive_staging(&cleanup_pool).await {
            warn!(%error, "Failed to clean inactive video processing staging directories");
        }
    });

    for worker_number in 0..*VIDEO_PROCESSING_CONCURRENCY {
        let pool = pool.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            info!(worker_number, "Started video processing worker");
            loop {
                match claim_job(&pool).await {
                    Ok(Some(job)) => process_claimed_job(&pool, &tx, job).await,
                    Ok(None) => time::sleep(WORKER_IDLE_DELAY).await,
                    Err(error) => {
                        error!(%error, "Failed to claim video processing job");
                        time::sleep(WORKER_IDLE_DELAY).await;
                    }
                }
            }
        });
    }
}

async fn cleanup_inactive_staging(pool: &PgPool) -> Result<(), String> {
    let root = processing_root();
    let mut entries = match fs::read_dir(&root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| error.to_string())?
    {
        if !entry
            .file_type()
            .await
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            continue;
        }
        let Some(job_id) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.split('-').next())
            .and_then(|name| name.parse::<i64>().ok())
        else {
            continue;
        };
        let active = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM media_processing_jobs WHERE id = $1 AND status IN ('queued', 'running'))",
        )
        .bind(job_id)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())?;
        if !active {
            fs::remove_dir_all(entry.path())
                .await
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

async fn claim_job(pool: &PgPool) -> Result<Option<VideoJob>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let lease_token = Uuid::new_v4().to_string();

    sqlx::query(
        r#"UPDATE media_processing_jobs
           SET status = 'queued', locked_at = NULL, lease_expires_at = NULL,
               lease_token = NULL, updated_at = now()
           WHERE status = 'running' AND lease_expires_at < now()"#,
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"UPDATE media_processing_jobs
           SET status = 'failed', finished_at = now(), updated_at = now()
           WHERE status = 'queued' AND attempts >= max_attempts"#,
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"UPDATE media SET processing_status = 'failed'
           FROM media_processing_jobs jobs
           WHERE media.id = jobs.media_id AND jobs.status = 'failed'
             AND jobs.attempts >= jobs.max_attempts"#,
    )
    .execute(&mut *transaction)
    .await?;

    let job_id = sqlx::query_scalar::<_, i64>(
        r#"WITH next_job AS (
                SELECT id
                FROM media_processing_jobs
                WHERE status = 'queued' AND attempts < max_attempts
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
    .bind(i64::try_from(*VIDEO_PROCESSING_LEASE_SECONDS).unwrap_or(120))
    .bind(&lease_token)
    .fetch_optional(&mut *transaction)
    .await?;

    let Some(job_id) = job_id else {
        transaction.commit().await?;
        return Ok(None);
    };

    let job = sqlx::query_as::<_, VideoJob>(
        r#"SELECT jobs.id, jobs.lease_token, jobs.media_id, media.filename, media.path,
                  media.type AS mime_type
           FROM media_processing_jobs jobs
           JOIN media ON media.id = jobs.media_id
           WHERE jobs.id = $1"#,
    )
    .bind(job_id)
    .fetch_optional(&mut *transaction)
    .await?;
    if let Some(job) = &job {
        sqlx::query("UPDATE media SET processing_status = 'processing' WHERE id = $1")
            .bind(job.media_id)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;
    Ok(job)
}

async fn process_claimed_job(pool: &PgPool, tx: &Sender<String>, job: VideoJob) {
    let _ = tx.send(
        SSEMessage::new(
            Level::Info,
            &format!("Video processing started: {}", job.filename),
        )
        .to_string(),
    );
    let lease_pool = pool.clone();
    let lease_job_id = job.id;
    let lease_token = job.lease_token.clone();
    let lease_task = tokio::spawn(async move {
        let every = Duration::from_secs((*VIDEO_PROCESSING_LEASE_SECONDS / 3).max(10));
        let mut interval = time::interval(every);
        interval.tick().await;
        loop {
            interval.tick().await;
            match renew_lease(&lease_pool, lease_job_id, &lease_token).await {
                Ok(true) => {}
                Ok(false) => {
                    warn!(job_id = lease_job_id, "Video processing lease was lost");
                    break;
                }
                Err(error) => {
                    warn!(job_id = lease_job_id, %error, "Failed to renew video processing lease");
                }
            }
        }
    });

    let media_id = job.media_id;
    let filename = job.filename.clone();
    let result = process_job(pool, tx, &job).await;
    lease_task.abort();

    match result {
        Ok(()) => {
            let _ = tx.send(
                SSEMessage::new(Level::Success, &format!("Video variants done: {filename}"))
                    .to_string(),
            );
        }
        Err(error) => {
            error!(job_id = job.id, media_id, %error, "Video processing failed");
            if let Err(update_error) =
                fail_job(pool, job.id, &job.lease_token, media_id, &error).await
            {
                error!(job_id = job.id, %update_error, "Failed to store video processing failure");
            }
            let _ = tx.send(
                SSEMessage::new(
                    Level::Error,
                    &format!("Video processing failed: {filename}"),
                )
                .to_string(),
            );
        }
    }
}

async fn renew_lease(pool: &PgPool, job_id: i64, lease_token: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE media_processing_jobs SET lease_expires_at = now() + ($1::BIGINT * interval '1 second'), updated_at = now() WHERE id = $2 AND lease_token = $3 AND status = 'running'",
    )
    .bind(i64::try_from(*VIDEO_PROCESSING_LEASE_SECONDS).unwrap_or(120))
    .bind(job_id)
    .bind(lease_token)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

async fn fail_job(
    pool: &PgPool,
    job_id: i64,
    lease_token: &str,
    media_id: i32,
    reason: &str,
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let attempts = sqlx::query_scalar::<_, bool>(
        "SELECT attempts >= max_attempts FROM media_processing_jobs WHERE id = $1 AND lease_token = $2 AND status = 'running'",
    )
    .bind(job_id)
    .bind(lease_token)
    .fetch_one(&mut *transaction)
    .await?;
    let status = if attempts { "failed" } else { "queued" };
    sqlx::query(
        "UPDATE media_processing_jobs SET status = $1, locked_at = NULL, lease_expires_at = NULL, lease_token = NULL, last_error = $2, finished_at = CASE WHEN $1 = 'failed' THEN now() ELSE NULL END, updated_at = now() WHERE id = $3 AND lease_token = $4 AND status = 'running'",
    )
    .bind(status)
    .bind(truncate_error(reason))
    .bind(job_id)
    .bind(lease_token)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("UPDATE media SET processing_status = $1 WHERE id = $2")
        .bind(if attempts { "failed" } else { "queued" })
        .bind(media_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await
}

async fn process_job(pool: &PgPool, tx: &Sender<String>, job: &VideoJob) -> Result<(), String> {
    if !job
        .mime_type
        .as_deref()
        .is_some_and(|mime| mime.starts_with("video/"))
    {
        return Err("The media record is not a video.".into());
    }

    let source = contained_storage_target(&job.path, &job.filename)
        .await
        .map_err(|error| error.to_string())?;
    if !fs::try_exists(&source)
        .await
        .map_err(|error| error.to_string())?
    {
        return Err("The uploaded video file no longer exists.".into());
    }
    let source_info = probe_video(&source).await?;
    let source_height =
        i32::try_from(source_info.height).map_err(|_| "Video height exceeds database range.")?;

    let configured_profiles = configured_profiles(pool).await?;
    let mut profiles: Vec<VideoProfile> = configured_profiles
        .iter()
        .filter(|profile| profile.height <= source_height)
        .cloned()
        .collect();
    if profiles.is_empty() {
        let mut fallback = configured_profiles
            .into_iter()
            .min_by_key(|profile| profile.height)
            .ok_or_else(|| "At least one video profile must be configured.".to_string())?;
        fallback.name = format!("{}-{}", fallback.name, source_height);
        fallback.height = source_height;
        fallback.cmd.retain(|arg| arg.flag != "-vf");
        profiles.push(fallback);
    }
    let staging_dir = processing_root().join(format!("{}-{}", job.id, job.lease_token));
    remove_stale_job_staging(job.id, &staging_dir).await?;
    if fs::try_exists(&staging_dir)
        .await
        .map_err(|error| error.to_string())?
    {
        fs::remove_dir_all(&staging_dir)
            .await
            .map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&staging_dir)
        .await
        .map_err(|error| error.to_string())?;

    let result = async {
        let stem = Path::new(&job.filename)
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Invalid video filename.".to_string())?;
        let mut variants = Vec::new();
        let mut generated_bytes = 0_u64;

        for profile in profiles {
            let filename = format!("{stem}--{}.{}", profile.name, profile.container);
            let staging_path = staging_dir.join(&filename);
            encode_variant(&source, &staging_path, &profile).await?;
            let info = probe_video(&staging_path).await?;
            validate_variant(&source_info, &info)?;
            let metadata = fs::metadata(&staging_path)
                .await
                .map_err(|error| error.to_string())?;
            generated_bytes = generated_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| "Generated output size exceeds the supported range.".to_string())?;
            let completed = ProcessedVariant {
                profile,
                filename,
                staging_path,
                width: i32::try_from(info.width)
                    .map_err(|_| "Video width exceeds database range.")?,
                height: i32::try_from(info.height)
                    .map_err(|_| "Video height exceeds database range.")?,
                size: i64::try_from(metadata.len()).map_err(|_| "Video variant is too large.")?,
                duration_ms: info.duration_ms,
            };
            let _ = tx.send(
                SSEMessage::new(
                    Level::Info,
                    &format!("Video variant compressed: {}", completed.filename),
                )
                .to_string(),
            );
            variants.push(completed);
        }
        if variants.is_empty() {
            return Err("No configured video profile fits the source dimensions.".into());
        }

        let (image_resolutions, image_extensions) = {
            let configuration = CONFIG.read().await;
            (
                configuration.image_resolutions.clone().unwrap_or_default(),
                configuration.image_extensions.clone().unwrap_or_default(),
            )
        };
        let thumbnails = create_thumbnails(
            &source,
            &staging_dir,
            stem,
            source_info.width,
            source_info.duration_ms.unwrap_or_default(),
            image_resolutions,
            image_extensions,
        )
        .await?;
        let mut transaction = lock_owned_job(pool, job).await?;
        ensure_output_targets_available(&mut transaction, job, &variants, &thumbnails).await?;
        publish_outputs(&job.path, &variants, &thumbnails).await?;
        persist_outputs(transaction, job, &variants, &thumbnails).await?;
        Ok(())
    }
    .await;

    if let Err(error) = fs::remove_dir_all(&staging_dir).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        warn!(path = %staging_dir.display(), %error, "Failed to remove video processing staging directory");
    }
    result
}

async fn remove_stale_job_staging(job_id: i64, current: &Path) -> Result<(), String> {
    let root = processing_root();
    let mut entries = match fs::read_dir(&root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    let prefix = format!("{job_id}-");
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| error.to_string())?
    {
        if entry.path() == current
            || !entry
                .file_type()
                .await
                .map_err(|error| error.to_string())?
                .is_dir()
            || !entry.file_name().to_string_lossy().starts_with(&prefix)
        {
            continue;
        }
        fs::remove_dir_all(entry.path())
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn lock_owned_job<'a>(
    pool: &'a PgPool,
    job: &VideoJob,
) -> Result<Transaction<'a, Postgres>, String> {
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let owned = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM media_processing_jobs WHERE id = $1 AND lease_token = $2 AND status = 'running' FOR UPDATE",
    )
    .bind(job.id)
    .bind(&job.lease_token)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    if owned.is_some() {
        Ok(transaction)
    } else {
        Err("Video processing lease was lost before publication.".into())
    }
}

async fn ensure_output_targets_available(
    transaction: &mut Transaction<'_, Postgres>,
    job: &VideoJob,
    variants: &[ProcessedVariant],
    thumbnails: &[ProcessedThumbnail],
) -> Result<(), String> {
    let mut names = HashSet::new();
    for filename in variants
        .iter()
        .map(|variant| variant.filename.as_str())
        .chain(
            thumbnails
                .iter()
                .map(|thumbnail| thumbnail.filename.as_str()),
        )
    {
        if !names.insert(filename) {
            return Err(format!(
                "Multiple video outputs use the filename '{filename}'."
            ));
        }

        let conflicts = sqlx::query_scalar::<_, bool>(
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
        .bind(&job.path)
        .bind(filename)
        .bind(job.media_id)
        .fetch_one(&mut **transaction)
        .await
        .map_err(|error| error.to_string())?;
        if conflicts {
            return Err(format!(
                "Generated output filename '{filename}' is already owned by another media item."
            ));
        }
    }
    Ok(())
}

fn validate_variant(source: &VideoInfo, variant: &VideoInfo) -> Result<(), String> {
    if variant.width > source.width || variant.height > source.height {
        return Err("Generated video variant unexpectedly upscales the source.".into());
    }
    let source_duration = source.duration_ms.unwrap_or_default();
    let variant_duration = variant.duration_ms.unwrap_or_default();
    let tolerance = (source_duration / 100).max(2_000);
    if source_duration.abs_diff(variant_duration) > u64::try_from(tolerance).unwrap_or(2_000) {
        return Err("Generated video variant has an unexpected duration.".into());
    }
    Ok(())
}

async fn create_thumbnails(
    source: &Path,
    staging_dir: &Path,
    stem: &str,
    source_width: u32,
    duration_ms: i64,
    image_resolutions: Vec<i32>,
    image_extensions: Vec<String>,
) -> Result<Vec<ProcessedThumbnail>, String> {
    if image_extensions.is_empty() {
        return Err("At least one image extension must be configured for video posters.".into());
    }

    let poster_source = staging_dir.join(format!("{stem}--poster.jpg"));
    create_thumbnail(source, &poster_source, source_width, duration_ms).await?;

    let variants = tokio::task::spawn_blocking({
        let poster_source = poster_source.clone();
        move || {
            save_image(image_resolutions, &image_extensions, &poster_source, None)
                .map_err(|error| error.to_string())
        }
    })
    .await
    .map_err(|error| error.to_string())??;

    fs::remove_file(&poster_source)
        .await
        .map_err(|error| error.to_string())?;

    if variants.is_empty() {
        return Err("No image variants were generated for the video poster.".into());
    }

    variants
        .into_iter()
        .map(|(width, height, filename)| {
            Ok(ProcessedThumbnail {
                staging_path: staging_dir.join(&filename),
                filename,
                width,
                height,
            })
        })
        .collect()
}

async fn publish_outputs(
    public_path: &str,
    variants: &[ProcessedVariant],
    thumbnails: &[ProcessedThumbnail],
) -> Result<(), String> {
    for variant in variants {
        publish_file(public_path, &variant.filename, &variant.staging_path).await?;
    }
    for thumbnail in thumbnails {
        publish_file(public_path, &thumbnail.filename, &thumbnail.staging_path).await?;
    }
    Ok(())
}

async fn publish_file(
    public_path: &str,
    filename: &str,
    staging_path: &Path,
) -> Result<(), String> {
    let target = contained_storage_target(public_path, filename)
        .await
        .map_err(|error| error.to_string())?;
    fs::rename(staging_path, &target)
        .await
        .map_err(|error| error.to_string())
}

async fn persist_outputs(
    mut transaction: Transaction<'_, Postgres>,
    job: &VideoJob,
    variants: &[ProcessedVariant],
    thumbnails: &[ProcessedThumbnail],
) -> Result<(), String> {
    let old_variant_filenames = sqlx::query_scalar::<_, String>(
        "SELECT filename FROM media_video_variants WHERE media_id = $1",
    )
    .bind(job.media_id)
    .fetch_all(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    for variant in variants {
        let video_codec =
            codec_from_cmd(&variant.profile.cmd, "-c:v").unwrap_or_else(|| "unknown".into());
        let audio_codec = codec_from_cmd(&variant.profile.cmd, "-c:a");
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
        .bind(&variant.profile.name)
        .bind(variant.width)
        .bind(variant.height)
        .bind(&variant.profile.container)
        .bind(&video_codec)
        .bind(&audio_codec)
        .bind(&variant.filename)
        .bind(variant.size)
        .bind(variant.duration_ms)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
    }

    let current_profiles: Vec<&str> = variants
        .iter()
        .map(|variant| variant.profile.name.as_str())
        .collect();
    sqlx::query("DELETE FROM media_video_variants WHERE media_id = $1 AND NOT (profile = ANY($2))")
        .bind(job.media_id)
        .bind(&current_profiles)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;

    let old_thumbnail_filenames =
        sqlx::query_scalar::<_, String>("SELECT filename FROM media_variants WHERE media_id = $1")
            .bind(job.media_id)
            .fetch_all(&mut *transaction)
            .await
            .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM media_variants WHERE media_id = $1")
        .bind(job.media_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
    for thumbnail in thumbnails {
        sqlx::query(
            r#"INSERT INTO media_variants (media_id, width, height, filename)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (media_id, width, height, filename) DO NOTHING"#,
        )
        .bind(job.media_id)
        .bind(thumbnail.width)
        .bind(thumbnail.height)
        .bind(&thumbnail.filename)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
    }

    let completed = sqlx::query(
        "UPDATE media_processing_jobs SET status = 'completed', finished_at = now(), locked_at = NULL, lease_expires_at = NULL, lease_token = NULL, last_error = NULL, updated_at = now() WHERE id = $1 AND lease_token = $2 AND status = 'running'",
    )
    .bind(job.id)
    .bind(&job.lease_token)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    if completed.rows_affected() != 1 {
        return Err("Video processing lease was lost during publication.".into());
    }
    sqlx::query("UPDATE media SET processing_status = 'completed' WHERE id = $1")
        .bind(job.media_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;

    let current_names: HashSet<&str> = variants
        .iter()
        .map(|variant| variant.filename.as_str())
        .chain(
            thumbnails
                .iter()
                .map(|thumbnail| thumbnail.filename.as_str()),
        )
        .collect();
    for filename in old_variant_filenames
        .into_iter()
        .chain(old_thumbnail_filenames)
        .filter(|filename| !current_names.contains(filename.as_str()))
    {
        let path = contained_storage_target(&job.path, &filename)
            .await
            .map_err(|error| error.to_string())?;
        if let Err(error) = fs::remove_file(&path).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            warn!(path = %path.display(), %error, "Failed to remove stale video output");
        }
    }
    Ok(())
}

async fn encode_variant(
    source: &Path,
    output: &Path,
    profile: &VideoProfile,
) -> Result<(), String> {
    let has_explicit_scale = profile.cmd.iter().any(|arg| arg.flag == "-vf");
    let cmd_args = profile_command_args(&profile.cmd);
    let auto_scale = if has_explicit_scale {
        Vec::new()
    } else {
        vec!["-vf".into(), format!("scale=-2:{}", profile.height)]
    };
    run_ffmpeg(
        [
            vec!["-i".into(), source.to_string_lossy().to_string()],
            vec!["-map".into(), "0:v:0".into(), "-map".into(), "0:a?".into()],
            cmd_args,
            auto_scale,
            vec!["-threads".into(), VIDEO_PROCESSING_THREADS.to_string()],
            vec![output.to_string_lossy().to_string()],
        ]
        .concat(),
    )
    .await
}

fn profile_command_args(cmd: &[VideoProfileArg]) -> Vec<String> {
    cmd.iter()
        .flat_map(|arg| {
            std::iter::once(arg.flag.clone())
                .chain((!arg.value.is_empty()).then(|| arg.value.clone()))
        })
        .collect()
}

/// Returns the value of the first `flag` argument configured on a profile,
/// used to populate the `media_video_variants.video_codec`/`audio_codec`
/// columns since the flat `cmd` list has no dedicated codec fields.
fn codec_from_cmd(cmd: &[VideoProfileArg], flag: &str) -> Option<String> {
    cmd.iter()
        .find(|arg| arg.flag == flag)
        .map(|arg| arg.value.clone())
}

async fn create_thumbnail(
    source: &Path,
    output: &Path,
    width: u32,
    duration_ms: i64,
) -> Result<(), String> {
    let seek_ms = (duration_ms / 10).clamp(0, 1_000);
    run_ffmpeg(vec![
        "-ss".into(),
        format!("{}.{:03}", seek_ms / 1_000, seek_ms % 1_000),
        "-i".into(),
        source.to_string_lossy().to_string(),
        "-map".into(),
        "0:v:0".into(),
        "-frames:v".into(),
        "1".into(),
        "-vf".into(),
        format!("scale={width}:-2"),
        "-q:v".into(),
        "2".into(),
        "-threads".into(),
        VIDEO_PROCESSING_THREADS.to_string(),
        output.to_string_lossy().to_string(),
    ])
    .await
}

async fn run_ffmpeg(args: Vec<String>) -> Result<(), String> {
    let mut command = Command::new(ffmpeg_bin());
    command
        .arg("-nostdin")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-filter_threads")
        .arg(VIDEO_PROCESSING_THREADS.to_string())
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = time::timeout(
        Duration::from_secs(*VIDEO_PROCESSING_TIMEOUT_SECONDS),
        command.output(),
    )
    .await
    .map_err(|_| "ffmpeg timed out.".to_string())?
    .map_err(|error| format!("Failed to start ffmpeg: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "ffmpeg exited with {}: {}",
        output.status,
        truncate_error(&String::from_utf8_lossy(&output.stderr))
    ))
}

async fn probe_video(path: &Path) -> Result<VideoInfo, String> {
    let mut command = Command::new(ffprobe_bin());
    command
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,width,height:format=duration",
            "-of",
            "json",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = time::timeout(PROBE_TIMEOUT, command.output())
        .await
        .map_err(|_| "ffprobe timed out.".to_string())?
        .map_err(|error| format!("Failed to start ffprobe: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "ffprobe exited with {}: {}",
            output.status,
            truncate_error(&String::from_utf8_lossy(&output.stderr))
        ));
    }
    let probe: ProbeResult = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Invalid ffprobe response: {error}"))?;
    let stream = probe
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("video"))
        .ok_or_else(|| "No video stream found.".to_string())?;
    let width = stream
        .width
        .ok_or_else(|| "Video width is missing.".to_string())?;
    let height = stream
        .height
        .ok_or_else(|| "Video height is missing.".to_string())?;
    let duration_ms = probe
        .format
        .and_then(|format| format.duration)
        .and_then(|duration| duration.parse::<f64>().ok())
        .filter(|duration| duration.is_finite() && *duration >= 0.0)
        .and_then(|duration| i64::try_from((duration * 1_000.0).round() as i128).ok());
    Ok(VideoInfo {
        width,
        height,
        duration_ms,
    })
}

async fn configured_profiles(pool: &PgPool) -> Result<Vec<VideoProfile>, String> {
    let profiles = handles::enabled_video_profiles(pool)
        .await
        .map_err(|error| error.to_string())?;
    if profiles.is_empty() {
        return Err("At least one video profile must be configured.".into());
    }
    for profile in &profiles {
        validate_video_profile(profile)?;
    }
    Ok(profiles)
}

/// Validates a video profile before it is persisted or used for processing.
/// Mirrors the safety checks the previous `NUR_VIDEO_PROFILES` env var relied
/// on: no shell is ever involved, but a malicious `cmd` entry could still
/// redirect ffmpeg's input/output or inject stray filter script files.
pub fn validate_video_profile(profile: &VideoProfile) -> Result<(), String> {
    if profile.height <= 0 || profile.name.is_empty() || profile.container.is_empty() {
        return Err("Video profile has required empty fields or a non-positive height.".into());
    }
    if !profile
        .name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        || !profile.container.chars().all(|c| c.is_ascii_alphanumeric())
    {
        return Err(format!(
            "Video profile '{}' has an unsafe name or container.",
            profile.name
        ));
    }
    if profile.name.len() > 64 || profile.container.len() > 16 || profile.cmd.len() > 64 {
        return Err("Video profile exceeds the configured field limits.".into());
    }
    if profile.container.eq_ignore_ascii_case("jpg") && profile.name.starts_with("thumbnail") {
        return Err("Video profile name conflicts with generated thumbnails.".into());
    }
    const VALUELESS_FLAGS: &[&str] = &["-an", "-sn", "-dn", "-shortest"];
    const ALLOWED_FLAGS: &[&str] = &[
        "-c:v",
        "-codec:v",
        "-vcodec",
        "-crf",
        "-preset",
        "-pix_fmt",
        "-vf",
        "-svtav1-params",
        "-x264-params",
        "-x265-params",
        "-c:a",
        "-codec:a",
        "-acodec",
        "-b:a",
        "-b:v",
        "-ar",
        "-ac",
        "-movflags",
        "-tag:v",
        "-quality",
        "-cpu-used",
        "-row-mt",
        "-deadline",
        "-tune",
        "-profile:v",
        "-level:v",
        "-g",
        "-keyint_min",
        "-an",
        "-sn",
        "-dn",
        "-shortest",
    ];
    for argument in &profile.cmd {
        if !ALLOWED_FLAGS.contains(&argument.flag.as_str()) {
            return Err(format!(
                "Video profile '{}' contains an unsupported ffmpeg flag '{}'.",
                profile.name, argument.flag
            ));
        }
        if argument.flag.len() > 32 || argument.value.len() > 512 {
            return Err(format!(
                "Video profile '{}' contains an oversized ffmpeg argument.",
                profile.name
            ));
        }
        let valueless = VALUELESS_FLAGS.contains(&argument.flag.as_str());
        if valueless != argument.value.is_empty() {
            return Err(format!(
                "Video profile '{}' has an invalid value for '{}'.",
                profile.name, argument.flag
            ));
        }
        if argument.value.contains('\0')
            || argument.value.contains("://")
            || argument.value.contains('/')
            || argument.value.contains('\\')
        {
            return Err(format!(
                "Video profile '{}' contains an unsafe ffmpeg value.",
                profile.name
            ));
        }
        if argument.flag == "-vf"
            && (!argument.value.starts_with("scale=")
                || !argument.value.chars().all(|character| {
                    character.is_ascii_alphanumeric()
                        || matches!(
                            character,
                            '=' | ':' | '-' | '+' | '*' | '.' | '_' | '(' | ')'
                        )
                }))
        {
            return Err(format!(
                "Video profile '{}' only supports a scale expression in '-vf'.",
                profile.name
            ));
        }
    }
    Ok(())
}

fn processing_root() -> PathBuf {
    PathBuf::from(STORAGE.as_str()).join(".processing")
}

fn ffmpeg_bin() -> String {
    env::var("NUR_FFMPEG_BIN").unwrap_or_else(|_| "ffmpeg".into())
}

fn ffprobe_bin() -> String {
    env::var("NUR_FFPROBE_BIN").unwrap_or_else(|_| "ffprobe".into())
}

fn truncate_error(error: &str) -> String {
    const MAX_ERROR_LEN: usize = 4_000;
    let mut value: String = error.chars().take(MAX_ERROR_LEN).collect();
    if error.chars().count() > MAX_ERROR_LEN {
        value.push('…');
    }
    value
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use crate::db::models::VideoProfileArg;

    use super::{
        VideoInfo, VideoProfile, enqueue_video_processing, profile_command_args,
        validate_video_profile,
    };

    const MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

    fn sample_profile() -> VideoProfile {
        VideoProfile {
            id: 0,
            name: "h264-480".into(),
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

    #[test]
    fn rejects_an_unsafe_profile_name() {
        let mut profile = sample_profile();
        profile.name = "../bad".into();
        assert!(validate_video_profile(&profile).is_err());
    }

    #[test]
    fn rejects_input_redirection_in_profiles() {
        let mut profile = sample_profile();
        profile.cmd = vec![VideoProfileArg {
            flag: "-i".into(),
            value: "/etc/passwd".into(),
        }];
        assert!(validate_video_profile(&profile).is_err());
    }

    #[test]
    fn rejects_a_cmd_entry_whose_flag_does_not_start_with_a_dash() {
        let mut profile = sample_profile();
        profile.cmd = vec![VideoProfileArg {
            flag: "c:v".into(),
            value: "libx264".into(),
        }];
        assert!(validate_video_profile(&profile).is_err());
    }

    #[test]
    fn rejects_a_non_positive_height() {
        let mut profile = sample_profile();
        profile.height = 0;
        assert!(validate_video_profile(&profile).is_err());
    }

    #[test]
    fn accepts_a_valid_profile() {
        assert!(validate_video_profile(&sample_profile()).is_ok());
    }

    #[test]
    fn keeps_valueless_flags_without_an_empty_positional_argument() {
        let cmd = vec![VideoProfileArg {
            flag: "-an".into(),
            value: String::new(),
        }];
        assert_eq!(profile_command_args(&cmd), ["-an"]);
    }

    #[test]
    fn rejects_flags_that_can_open_an_additional_url() {
        let mut profile = sample_profile();
        profile.cmd.push(VideoProfileArg {
            flag: "-progress".into(),
            value: "https://example.invalid".into(),
        });
        assert!(validate_video_profile(&profile).is_err());
    }

    #[test]
    fn source_dimensions_are_kept_as_unsigned_values() {
        let info = VideoInfo {
            width: 1_920,
            height: 1_080,
            duration_ms: Some(1_000),
        };
        assert_eq!(info.width * info.height, 2_073_600);
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn does_not_reset_an_active_job_when_enqueued_twice(pool: PgPool) {
        let media_id: i32 = sqlx::query_scalar(
            "INSERT INTO media (filename, path, type) VALUES ('video.mp4', '/uploads', 'video/mp4') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("video can be inserted");
        enqueue_video_processing(&pool, media_id)
            .await
            .expect("first enqueue succeeds");
        let second = enqueue_video_processing(&pool, media_id).await;
        assert!(matches!(
            second,
            Err(crate::utils::errors::NurError::Conflict(_))
        ));
        let jobs: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM media_processing_jobs WHERE media_id = $1 AND status = 'queued'",
        )
        .bind(media_id)
        .fetch_one(&pool)
        .await
        .expect("jobs can be counted");
        assert_eq!(jobs, 1);
    }
}
