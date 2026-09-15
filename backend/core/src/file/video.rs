use std::{
    collections::HashSet,
    env,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

use rand::RngExt;
use serde::Deserialize;
use sqlx::{FromRow, Postgres, Transaction, postgres::PgPool};
use tokio::{
    fs,
    process::Command,
    sync::{OnceCell, broadcast::Sender, watch},
    task::JoinHandle,
    time,
};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    CONFIG, ENTRY_CACHE, STORAGE, VIDEO_PROCESSING_CONCURRENCY, VIDEO_PROCESSING_LEASE_SECONDS,
    VIDEO_PROCESSING_MAX_ATTEMPTS, VIDEO_PROCESSING_MAX_DURATION_SECONDS,
    VIDEO_PROCESSING_MAX_OUTPUT_SIZE, VIDEO_PROCESSING_MAX_PIXELS, VIDEO_PROCESSING_THREADS,
    VIDEO_PROCESSING_TIMEOUT_SECONDS,
    db::{
        handles,
        models::{VideoProfile, VideoProfileArg},
    },
    file::{helper::contained_storage_target, processing::save_image},
    sse::{SSELevel as Level, SSEMessage},
    utils::errors::NurError,
};

const JOB_KIND: &str = "video_variants";
const MANUAL_THUMBNAIL_JOB_KIND: &str = "video_thumbnail_manual";
const RANDOM_THUMBNAIL_JOB_KIND: &str = "video_thumbnail_random";
const WORKER_IDLE_DELAY: Duration = Duration::from_secs(2);
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
const RETRY_BASE_DELAY_SECONDS: i64 = 30;
const PROCESSING_CANCELLED: &str = "Video processing was cancelled.";
const PROCESSING_LEASE_LOST: &str = "Video processing lease was lost.";
static AVAILABLE_ENCODERS: OnceCell<HashSet<String>> = OnceCell::const_new();

#[derive(Clone)]
struct ProcessingControl {
    shutdown: watch::Receiver<bool>,
    lease_lost: watch::Receiver<bool>,
}

impl ProcessingControl {
    async fn cancelled(&self) {
        let mut shutdown = self.shutdown.clone();
        let mut lease_lost = self.lease_lost.clone();
        tokio::select! {
            _ = shutdown.wait_for(|value| *value) => {},
            _ = lease_lost.wait_for(|value| *value) => {},
        }
    }
}

pub struct VideoWorkers {
    shutdown: watch::Sender<bool>,
    tasks: Vec<JoinHandle<()>>,
}

impl VideoWorkers {
    pub fn shutdown_sender(&self) -> watch::Sender<bool> {
        self.shutdown.clone()
    }

    pub async fn wait(self) {
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

#[derive(Debug, FromRow)]
struct VideoJob {
    id: i64,
    attempts: i32,
    max_attempts: i32,
    lease_token: String,
    media_id: i32,
    filename: String,
    path: String,
    mime_type: Option<String>,
    kind: String,
    source_media_id: Option<i32>,
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
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
    format_name: Option<String>,
}

#[derive(Debug)]
struct VideoInfo {
    width: u32,
    height: u32,
    duration_ms: Option<i64>,
    format_name: Option<String>,
    video_codec: Option<String>,
    audio_codec: Option<String>,
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

struct ThumbnailOutputConfig {
    resolutions: Vec<i32>,
    extensions: Vec<String>,
}

#[derive(Debug)]
struct PublishedFile {
    target: PathBuf,
    backup: Option<PathBuf>,
}

pub async fn enqueue_video_processing(pool: &PgPool, media_id: i32) -> Result<(), NurError> {
    enqueue_video_job(pool, media_id, JOB_KIND, None).await
}

pub async fn ensure_video_processing(pool: &PgPool, media_id: i32) -> Result<(), NurError> {
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
    .bind(JOB_KIND)
    .bind(*VIDEO_PROCESSING_MAX_ATTEMPTS)
    .execute(&mut *transaction)
    .await?;
    if inserted.rows_affected() == 1 {
        sqlx::query("UPDATE media SET processing_status = 'queued' WHERE id = $1")
            .bind(media_id)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;
    Ok(())
}

pub async fn enqueue_video_thumbnail(
    pool: &PgPool,
    media_id: i32,
    source_media_id: Option<i32>,
) -> Result<(), NurError> {
    let kind = if source_media_id.is_some() {
        MANUAL_THUMBNAIL_JOB_KIND
    } else {
        RANDOM_THUMBNAIL_JOB_KIND
    };
    enqueue_video_job(pool, media_id, kind, source_media_id).await
}

fn is_thumbnail_job(kind: &str) -> bool {
    matches!(kind, MANUAL_THUMBNAIL_JOB_KIND | RANDOM_THUMBNAIL_JOB_KIND)
}

async fn enqueue_video_job(
    pool: &PgPool,
    media_id: i32,
    kind: &str,
    source_media_id: Option<i32>,
) -> Result<(), NurError> {
    let mut transaction = pool.begin().await?;
    let inserted = sqlx::query(
        r#"INSERT INTO media_processing_jobs (media_id, kind, source_media_id, max_attempts)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (media_id) WHERE status IN ('queued', 'running') DO NOTHING"#,
    )
    .bind(media_id)
    .bind(kind)
    .bind(source_media_id)
    .bind(*VIDEO_PROCESSING_MAX_ATTEMPTS)
    .execute(&mut *transaction)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(NurError::Conflict(
            "Video processing is already queued or running.".into(),
        ));
    }
    if kind == JOB_KIND {
        sqlx::query("UPDATE media SET processing_status = 'queued' WHERE id = $1")
            .bind(media_id)
            .execute(&mut *transaction)
            .await?;
    }
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
pub fn start_video_workers(pool: PgPool, tx: Sender<String>) -> VideoWorkers {
    let (shutdown, _) = watch::channel(false);
    let mut tasks = Vec::new();
    let cleanup_pool = pool.clone();
    tasks.push(tokio::spawn(async move {
        if let Err(error) = recover_queued_video_jobs(&cleanup_pool).await {
            warn!(%error, "Failed to recover queued video processing jobs");
        }
        if let Err(error) = cleanup_inactive_staging(&cleanup_pool).await {
            warn!(%error, "Failed to clean inactive video processing staging directories");
        }
    }));

    for worker_number in 0..*VIDEO_PROCESSING_CONCURRENCY {
        let pool = pool.clone();
        let tx = tx.clone();
        let mut shutdown_rx = shutdown.subscribe();
        tasks.push(tokio::spawn(async move {
            info!(worker_number, "Started video processing worker");
            loop {
                let claim = tokio::select! {
                    _ = shutdown_rx.wait_for(|value| *value) => break,
                    result = claim_job(&pool) => result,
                };
                match claim {
                    Ok(Some(job)) => {
                        process_claimed_job(&pool, &tx, job, shutdown_rx.clone()).await;
                    }
                    Ok(None) => tokio::select! {
                        _ = shutdown_rx.wait_for(|value| *value) => break,
                        _ = time::sleep(WORKER_IDLE_DELAY) => {},
                    },
                    Err(error) => {
                        error!(%error, "Failed to claim video processing job");
                        tokio::select! {
                            _ = shutdown_rx.wait_for(|value| *value) => break,
                            _ = time::sleep(WORKER_IDLE_DELAY) => {},
                        }
                    }
                }
            }
            info!(worker_number, "Stopped video processing worker");
        }));
    }
    VideoWorkers { shutdown, tasks }
}

async fn recover_queued_video_jobs(pool: &PgPool) -> Result<(), String> {
    let media = sqlx::query_as::<_, (i32, String, String)>(
        r#"SELECT id, path, filename
           FROM media
           WHERE type LIKE 'video/%' AND processing_status = 'queued'
             AND NOT EXISTS (
                 SELECT 1 FROM media_processing_jobs jobs
                 WHERE jobs.media_id = media.id AND jobs.status IN ('queued', 'running')
             )"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    for (media_id, path, filename) in media {
        let source = contained_storage_target(&path, &filename)
            .await
            .map_err(|error| error.to_string())?;
        if fs::try_exists(source)
            .await
            .map_err(|error| error.to_string())?
        {
            ensure_video_processing(pool, media_id)
                .await
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
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
    .bind(JOB_KIND)
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
    .bind(i64::try_from(*VIDEO_PROCESSING_LEASE_SECONDS).unwrap_or(120))
    .bind(&lease_token)
    .bind(RETRY_BASE_DELAY_SECONDS)
    .fetch_optional(&mut *transaction)
    .await?;

    let Some(job_id) = job_id else {
        transaction.commit().await?;
        return Ok(None);
    };

    let job = sqlx::query_as::<_, VideoJob>(
        r#"SELECT jobs.id, jobs.attempts, jobs.max_attempts, jobs.lease_token, jobs.media_id, media.filename, media.path,
                  media.type AS mime_type, jobs.kind, jobs.source_media_id
           FROM media_processing_jobs jobs
           JOIN media ON media.id = jobs.media_id
           WHERE jobs.id = $1"#,
    )
    .bind(job_id)
    .fetch_optional(&mut *transaction)
    .await?;
    if let Some(job) = &job {
        if job.kind == JOB_KIND {
            sqlx::query("UPDATE media SET processing_status = 'processing' WHERE id = $1")
                .bind(job.media_id)
                .execute(&mut *transaction)
                .await?;
        }
        let queued_jobs = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM media_processing_jobs WHERE status = 'queued' AND attempts < max_attempts",
        )
        .fetch_one(&mut *transaction)
        .await?;
        info!(
            job_id = job.id,
            media_id = job.media_id,
            attempt = job.attempts,
            queued_jobs,
            "Claimed video processing job"
        );
    }
    transaction.commit().await?;
    Ok(job)
}

async fn process_claimed_job(
    pool: &PgPool,
    tx: &Sender<String>,
    job: VideoJob,
    shutdown: watch::Receiver<bool>,
) {
    let started = Instant::now();
    let action = if is_thumbnail_job(&job.kind) {
        "thumbnail generation"
    } else {
        "video processing"
    };
    let _ = tx.send(
        SSEMessage::new(Level::Info, &format!("{action} started: {}", job.filename))
            .with_media_id(job.media_id)
            .to_string(),
    );
    let lease_pool = pool.clone();
    let lease_job_id = job.id;
    let lease_token = job.lease_token.clone();
    let (lease_lost_tx, lease_lost) = watch::channel(false);
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
                    let _ = lease_lost_tx.send(true);
                    break;
                }
                Err(error) => {
                    warn!(job_id = lease_job_id, %error, "Failed to renew video processing lease");
                    let _ = lease_lost_tx.send(true);
                    break;
                }
            }
        }
    });

    let media_id = job.media_id;
    let filename = job.filename.clone();
    let control = ProcessingControl {
        shutdown,
        lease_lost,
    };
    let result = match job.kind.as_str() {
        JOB_KIND => process_job(pool, tx, &job, control.clone()).await,
        MANUAL_THUMBNAIL_JOB_KIND | RANDOM_THUMBNAIL_JOB_KIND => {
            process_thumbnail_job(pool, &job, control).await
        }
        _ => Err("Unknown video processing job kind.".into()),
    };
    lease_task.abort();

    match result {
        Ok(()) => {
            ENTRY_CACHE.invalidate();
            info!(
                job_id = job.id,
                media_id,
                kind = %job.kind,
                attempt = job.attempts,
                duration_ms = started.elapsed().as_millis(),
                "Video processing job completed"
            );
            let completed = if is_thumbnail_job(&job.kind) {
                "Video thumbnail done"
            } else {
                "Video variants done"
            };
            let _ = tx.send(
                SSEMessage::new(Level::Success, &format!("{completed}: {filename}"))
                    .with_media_id(media_id)
                    .to_string(),
            );
        }
        Err(error) => {
            if matches!(error.as_str(), PROCESSING_CANCELLED | PROCESSING_LEASE_LOST) {
                if error == PROCESSING_LEASE_LOST {
                    warn!(
                        job_id = job.id,
                        media_id, "Stopped processing after lease loss"
                    );
                    return;
                }
                if let Err(update_error) = release_cancelled_job(pool, &job).await
                    && !matches!(update_error, sqlx::Error::RowNotFound)
                {
                    error!(job_id = job.id, %update_error, "Failed to release cancelled video job");
                }
                return;
            }
            let retryable = is_retryable_processing_error(&error);
            let will_retry = retryable && job.attempts < job.max_attempts;
            error!(
                job_id = job.id,
                media_id,
                kind = %job.kind,
                attempt = job.attempts,
                duration_ms = started.elapsed().as_millis(),
                retryable,
                will_retry,
                %error,
                "Video processing failed"
            );
            if let Err(update_error) = fail_job(pool, &job, &error, retryable).await {
                if !matches!(update_error, sqlx::Error::RowNotFound) {
                    error!(job_id = job.id, %update_error, "Failed to store video processing failure");
                }
                return;
            }
            let failed = if will_retry && is_thumbnail_job(&job.kind) {
                "Video thumbnail retry queued"
            } else if will_retry {
                "Video processing retry queued"
            } else if is_thumbnail_job(&job.kind) {
                "Video thumbnail failed"
            } else {
                "Video processing failed"
            };
            let _ = tx.send(
                SSEMessage::new(
                    if will_retry {
                        Level::Warning
                    } else {
                        Level::Error
                    },
                    &format!("{failed}: {filename}"),
                )
                .with_media_id(media_id)
                .to_string(),
            );
        }
    }
}

async fn release_cancelled_job(pool: &PgPool, job: &VideoJob) -> Result<(), sqlx::Error> {
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
    if job.kind == JOB_KIND {
        sqlx::query("UPDATE media SET processing_status = 'queued' WHERE id = $1")
            .bind(job.media_id)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await
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
    job: &VideoJob,
    reason: &str,
    retryable: bool,
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let attempts = sqlx::query_scalar::<_, bool>(
        "SELECT attempts >= max_attempts FROM media_processing_jobs WHERE id = $1 AND lease_token = $2 AND status = 'running' FOR UPDATE",
    )
    .bind(job.id)
    .bind(&job.lease_token)
    .fetch_one(&mut *transaction)
    .await?;
    let will_retry = retryable && !attempts;
    let status = if will_retry { "queued" } else { "failed" };
    let updated = sqlx::query(
        "UPDATE media_processing_jobs SET status = $1, locked_at = NULL, lease_expires_at = NULL, lease_token = NULL, last_error = $2, finished_at = CASE WHEN $1 = 'failed' THEN now() ELSE NULL END, updated_at = now() WHERE id = $3 AND lease_token = $4 AND status = 'running'",
    )
    .bind(status)
    .bind(truncate_error(reason))
    .bind(job.id)
    .bind(&job.lease_token)
    .execute(&mut *transaction)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(sqlx::Error::RowNotFound);
    }
    let media_status = media_status_after_failure(&job.kind, !will_retry);
    sqlx::query("UPDATE media SET processing_status = $1 WHERE id = $2")
        .bind(media_status)
        .bind(job.media_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await
}

fn is_retryable_processing_error(reason: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    [
        "timed out",
        "pool timed out",
        "connection refused",
        "connection reset",
        "connection closed",
        "temporarily unavailable",
        "too many open files",
        "out of memory",
        "database is starting up",
    ]
    .iter()
    .any(|fragment| reason.contains(fragment))
}

fn media_status_after_failure(kind: &str, terminal: bool) -> &'static str {
    if is_thumbnail_job(kind) {
        "completed"
    } else if terminal {
        "failed"
    } else {
        "queued"
    }
}

async fn process_job(
    pool: &PgPool,
    tx: &Sender<String>,
    job: &VideoJob,
    control: ProcessingControl,
) -> Result<(), String> {
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
    validate_source(&source_info, job.mime_type.as_deref().unwrap_or_default())?;
    let source_height =
        i32::try_from(source_info.height).map_err(|_| "Video height exceeds database range.")?;

    let configured_profiles = configured_profiles(pool).await?;
    let profiles = select_profiles(configured_profiles, source_height)?;
    validate_available_encoders(&profiles).await?;
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
            let filename = variant_filename(stem, &profile);
            let staging_path = staging_dir.join(&filename);
            encode_variant(&source, &staging_path, &profile, control.clone()).await?;
            let info = probe_video(&staging_path).await?;
            validate_variant(&source_info, &info, &profile)?;
            let metadata = fs::metadata(&staging_path)
                .await
                .map_err(|error| error.to_string())?;
            generated_bytes = generated_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| "Generated output size exceeds the supported range.".to_string())?;
            if let Some(limit) = *VIDEO_PROCESSING_MAX_OUTPUT_SIZE
                && (metadata.len() > limit || generated_bytes > limit)
            {
                return Err(format!(
                    "Generated video output exceeds the configured {} byte limit.",
                    limit
                ));
            }
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
                .with_media_id(job.media_id)
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
            ThumbnailOutputConfig {
                resolutions: image_resolutions,
                extensions: image_extensions,
            },
            control,
        )
        .await?;
        for thumbnail in &thumbnails {
            let size = fs::metadata(&thumbnail.staging_path)
                .await
                .map_err(|error| error.to_string())?
                .len();
            generated_bytes = generated_bytes
                .checked_add(size)
                .ok_or_else(|| "Generated output size exceeds the supported range.".to_string())?;
            if let Some(limit) = *VIDEO_PROCESSING_MAX_OUTPUT_SIZE
                && generated_bytes > limit
            {
                return Err(format!(
                    "Generated video output exceeds the configured {} byte limit.",
                    limit
                ));
            }
        }
        let mut transaction = lock_owned_job(pool, job).await?;
        ensure_output_targets_available(&mut transaction, job, &variants, &thumbnails).await?;
        let published = publish_outputs(&job.path, &variants, &thumbnails).await?;
        if let Err(error) = persist_outputs(transaction, job, &variants, &thumbnails).await {
            remove_published_outputs(&published).await;
            return Err(error);
        }
        remove_publication_backups(&published).await;
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

fn variant_filename(stem: &str, profile: &VideoProfile) -> String {
    format!("{stem}--{}.{}", profile.name, profile.container)
}

fn select_profiles(
    configured_profiles: Vec<VideoProfile>,
    source_height: i32,
) -> Result<Vec<VideoProfile>, String> {
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
        fallback.height = source_height;
        fallback.cmd.retain(|arg| arg.flag != "-vf");
        profiles.push(fallback);
    }
    Ok(profiles)
}

async fn process_thumbnail_job(
    pool: &PgPool,
    job: &VideoJob,
    control: ProcessingControl,
) -> Result<(), String> {
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
    let video_info = probe_video(&source).await?;
    validate_source(&video_info, job.mime_type.as_deref().unwrap_or_default())?;
    let stem = Path::new(&job.filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Invalid video filename.".to_string())?;
    let staging_dir = processing_root().join(format!("{}-{}", job.id, job.lease_token));
    remove_stale_job_staging(job.id, &staging_dir).await?;
    fs::create_dir_all(&staging_dir)
        .await
        .map_err(|error| error.to_string())?;

    let result = async {
        let poster_source = match (job.kind.as_str(), job.source_media_id) {
            (MANUAL_THUMBNAIL_JOB_KIND, Some(source_media_id)) => {
                let (filename, path, mime_type): (String, String, Option<String>) =
                    sqlx::query_as("SELECT filename, path, type FROM media WHERE id = $1")
                        .bind(source_media_id)
                        .fetch_optional(pool)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| {
                            "The selected thumbnail image no longer exists.".to_string()
                        })?;
                let extension = image_extension(mime_type.as_deref())
                    .ok_or_else(|| "The selected thumbnail must be a raster image.".to_string())?;
                let thumbnail_source = contained_storage_target(&path, &filename)
                    .await
                    .map_err(|error| error.to_string())?;
                if !fs::try_exists(&thumbnail_source)
                    .await
                    .map_err(|error| error.to_string())?
                {
                    return Err("The selected thumbnail image file no longer exists.".into());
                }
                let poster_source = staging_dir.join(format!("{stem}--thumbnail.{extension}"));
                fs::copy(&thumbnail_source, &poster_source)
                    .await
                    .map_err(|error| error.to_string())?;
                poster_source
            }
            (RANDOM_THUMBNAIL_JOB_KIND, None) => {
                let seek_ms = random_thumbnail_seek(video_info.duration_ms)?;
                let poster_source = staging_dir.join(format!("{stem}--thumbnail.jpg"));
                create_thumbnail_at(
                    &source,
                    &poster_source,
                    video_info.width,
                    seek_ms,
                    control.clone(),
                )
                .await?;
                poster_source
            }
            (MANUAL_THUMBNAIL_JOB_KIND, None) => {
                return Err("The selected thumbnail image no longer exists.".into());
            }
            _ => return Err("Invalid video thumbnail job payload.".into()),
        };
        let (image_resolutions, image_extensions) = {
            let configuration = CONFIG.read().await;
            (
                configuration.image_resolutions.clone().unwrap_or_default(),
                configuration.image_extensions.clone().unwrap_or_default(),
            )
        };
        let thumbnails =
            create_thumbnail_variants(&poster_source, image_resolutions, image_extensions).await?;
        let mut generated_bytes = 0_u64;
        for thumbnail in &thumbnails {
            generated_bytes = generated_bytes
                .checked_add(
                    fs::metadata(&thumbnail.staging_path)
                        .await
                        .map_err(|error| error.to_string())?
                        .len(),
                )
                .ok_or_else(|| "Generated output size exceeds the supported range.".to_string())?;
        }
        if let Some(limit) = *VIDEO_PROCESSING_MAX_OUTPUT_SIZE
            && generated_bytes > limit
        {
            return Err(format!(
                "Generated video thumbnail output exceeds the configured {} byte limit.",
                limit
            ));
        }
        fs::remove_file(&poster_source)
            .await
            .map_err(|error| error.to_string())?;

        let mut transaction = lock_owned_job(pool, job).await?;
        ensure_output_targets_available(&mut transaction, job, &[], &thumbnails).await?;
        let published = publish_outputs(&job.path, &[], &thumbnails).await?;
        if let Err(error) = persist_thumbnail_outputs(transaction, job, &thumbnails).await {
            remove_published_outputs(&published).await;
            return Err(error);
        }
        remove_publication_backups(&published).await;
        Ok(())
    }
    .await;

    if let Err(error) = fs::remove_dir_all(&staging_dir).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        warn!(path = %staging_dir.display(), %error, "Failed to remove video thumbnail staging directory");
    }
    result
}

fn image_extension(mime_type: Option<&str>) -> Option<&'static str> {
    match mime_type {
        Some("image/avif") => Some("avif"),
        Some("image/gif") => Some("gif"),
        Some("image/jpeg" | "image/jpg") => Some("jpg"),
        Some("image/png") => Some("png"),
        Some("image/webp") => Some("webp"),
        _ => None,
    }
}

fn random_thumbnail_seek(duration_ms: Option<i64>) -> Result<i64, String> {
    let duration_ms = duration_ms
        .filter(|duration| *duration > 0)
        .ok_or_else(|| {
            "The video duration is unavailable; a random thumbnail cannot be generated.".to_string()
        })?;
    if duration_ms <= 250 {
        return Ok(0);
    }

    let margin = (duration_ms / 20).clamp(100, 5_000);
    let end = duration_ms.saturating_sub(margin);
    if end <= margin {
        return Ok(duration_ms / 2);
    }

    Ok(rand::rng().random_range(margin..end))
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
        Err(PROCESSING_LEASE_LOST.into())
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

fn validate_variant(
    source: &VideoInfo,
    variant: &VideoInfo,
    profile: &VideoProfile,
) -> Result<(), String> {
    if variant.width > source.width || variant.height > source.height {
        return Err("Generated video variant unexpectedly upscales the source.".into());
    }
    if variant.width == 0 || variant.height == 0 {
        return Err("Generated video dimensions must be positive.".into());
    }
    let source_duration = source
        .duration_ms
        .filter(|duration| *duration > 0)
        .ok_or_else(|| "Source video duration is missing or invalid.".to_string())?;
    let variant_duration = variant
        .duration_ms
        .filter(|duration| *duration > 0)
        .ok_or_else(|| "Generated video duration is missing or invalid.".to_string())?;
    let tolerance = (source_duration / 100).max(2_000);
    if source_duration.abs_diff(variant_duration) > u64::try_from(tolerance).unwrap_or(2_000) {
        return Err("Generated video variant has an unexpected duration.".into());
    }
    if profile
        .cmd
        .iter()
        .find(|argument| argument.flag == "-pix_fmt")
        .is_some_and(|argument| argument.value.starts_with("yuv420"))
        && (!variant.width.is_multiple_of(2) || !variant.height.is_multiple_of(2))
    {
        return Err("Generated chroma-subsampled video dimensions must be even.".into());
    }
    if !format_matches_container(variant.format_name.as_deref(), &profile.container) {
        return Err("Generated video container does not match its profile.".into());
    }
    if let Some(expected) = codec_from_cmd_any(&profile.cmd, &["-c:v", "-codec:v", "-vcodec"])
        && expected != "copy"
        && variant.video_codec.as_deref() != Some(codec_name_for_encoder(&expected))
    {
        return Err("Generated video codec does not match its profile.".into());
    }
    if source.audio_codec.is_some()
        && let Some(expected) = codec_from_cmd_any(&profile.cmd, &["-c:a", "-codec:a", "-acodec"])
        && expected != "copy"
        && variant.audio_codec.as_deref() != Some(codec_name_for_encoder(&expected))
    {
        return Err("Generated audio codec does not match its profile.".into());
    }
    Ok(())
}

fn codec_name_for_encoder(encoder: &str) -> &str {
    if encoder.contains("264") {
        "h264"
    } else if encoder.contains("265") || encoder.contains("hevc") {
        "hevc"
    } else if encoder.contains("av1") || encoder == "librav1e" {
        "av1"
    } else if encoder.contains("vp9") {
        "vp9"
    } else if encoder == "libvpx" || encoder.contains("vp8") {
        "vp8"
    } else if encoder.contains("aac") {
        "aac"
    } else if encoder.contains("opus") {
        "opus"
    } else if encoder.contains("vorbis") {
        "vorbis"
    } else {
        encoder
    }
}

async fn create_thumbnails(
    source: &Path,
    staging_dir: &Path,
    stem: &str,
    source_width: u32,
    duration_ms: i64,
    output: ThumbnailOutputConfig,
    control: ProcessingControl,
) -> Result<Vec<ProcessedThumbnail>, String> {
    if output.extensions.is_empty() {
        return Err("At least one image extension must be configured for video posters.".into());
    }

    let poster_source = staging_dir.join(format!("{stem}--thumbnail.jpg"));
    create_thumbnail(source, &poster_source, source_width, duration_ms, control).await?;

    let variants =
        create_thumbnail_variants(&poster_source, output.resolutions, output.extensions).await?;

    fs::remove_file(&poster_source)
        .await
        .map_err(|error| error.to_string())?;

    Ok(variants)
}

async fn create_thumbnail_variants(
    poster_source: &Path,
    image_resolutions: Vec<i32>,
    image_extensions: Vec<String>,
) -> Result<Vec<ProcessedThumbnail>, String> {
    if image_extensions.is_empty() {
        return Err("At least one image extension must be configured for video posters.".into());
    }
    let variants = tokio::task::spawn_blocking({
        let poster_source = poster_source.to_path_buf();
        move || {
            save_image(image_resolutions, &image_extensions, &poster_source, None)
                .map_err(|error| error.to_string())
        }
    })
    .await
    .map_err(|error| error.to_string())??;

    if variants.is_empty() {
        return Err("No image variants were generated for the video poster.".into());
    }

    variants
        .into_iter()
        .map(|(width, height, filename)| {
            Ok(ProcessedThumbnail {
                staging_path: poster_source.with_file_name(&filename),
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
) -> Result<Vec<PublishedFile>, String> {
    let mut published = Vec::with_capacity(variants.len() + thumbnails.len());
    for variant in variants {
        match publish_file(public_path, &variant.filename, &variant.staging_path).await {
            Ok(file) => published.push(file),
            Err(error) => {
                remove_published_outputs(&published).await;
                return Err(error);
            }
        }
    }
    for thumbnail in thumbnails {
        match publish_file(public_path, &thumbnail.filename, &thumbnail.staging_path).await {
            Ok(file) => published.push(file),
            Err(error) => {
                remove_published_outputs(&published).await;
                return Err(error);
            }
        }
    }
    Ok(published)
}

async fn publish_file(
    public_path: &str,
    filename: &str,
    staging_path: &Path,
) -> Result<PublishedFile, String> {
    let target = contained_storage_target(public_path, filename)
        .await
        .map_err(|error| error.to_string())?;
    publish_staged_file(staging_path, &target).await
}

async fn publish_staged_file(staging_path: &Path, target: &Path) -> Result<PublishedFile, String> {
    let backup = if fs::try_exists(&target)
        .await
        .map_err(|error| error.to_string())?
    {
        let backup = staging_path.with_file_name(format!(".replaced-{}", Uuid::new_v4()));
        if fs::hard_link(target, &backup).await.is_err() {
            fs::copy(target, &backup)
                .await
                .map_err(|error| error.to_string())?;
        }
        Some(backup)
    } else {
        None
    };
    if let Err(error) = fs::rename(staging_path, target).await {
        if let Some(backup) = &backup
            && let Err(remove_error) = fs::remove_file(backup).await
        {
            error!(%remove_error, "Failed to remove unused video output backup");
        }
        return Err(error.to_string());
    }
    Ok(PublishedFile {
        target: target.to_path_buf(),
        backup,
    })
}

async fn remove_published_outputs(files: &[PublishedFile]) {
    for file in files.iter().rev() {
        if let Err(error) = fs::remove_file(&file.target).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            warn!(path = %file.target.display(), %error, "Failed to roll back published video output");
        }
        if let Some(backup) = &file.backup
            && let Err(error) = fs::rename(backup, &file.target).await
        {
            warn!(path = %file.target.display(), %error, "Failed to restore previous video output");
        }
    }
}

async fn remove_publication_backups(files: &[PublishedFile]) {
    for file in files {
        if let Some(backup) = &file.backup
            && let Err(error) = fs::remove_file(backup).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            warn!(path = %backup.display(), %error, "Failed to remove replaced video output backup");
        }
    }
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
            codec_from_cmd_any(&variant.profile.cmd, &["-c:v", "-codec:v", "-vcodec"])
                .unwrap_or_else(|| "unknown".into());
        let audio_codec =
            codec_from_cmd_any(&variant.profile.cmd, &["-c:a", "-codec:a", "-acodec"]);
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
        return Err(PROCESSING_LEASE_LOST.into());
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
        let path = match contained_storage_target(&job.path, &filename).await {
            Ok(path) => path,
            Err(error) => {
                warn!(filename, %error, "Failed to resolve stale video output");
                continue;
            }
        };
        if let Err(error) = fs::remove_file(&path).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            warn!(path = %path.display(), %error, "Failed to remove stale video output");
        }
    }
    Ok(())
}

async fn persist_thumbnail_outputs(
    mut transaction: Transaction<'_, Postgres>,
    job: &VideoJob,
    thumbnails: &[ProcessedThumbnail],
) -> Result<(), String> {
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
               VALUES ($1, $2, $3, $4)"#,
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
        return Err(PROCESSING_LEASE_LOST.into());
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

    let current_names: HashSet<&str> = thumbnails
        .iter()
        .map(|thumbnail| thumbnail.filename.as_str())
        .collect();
    for filename in old_thumbnail_filenames
        .into_iter()
        .filter(|filename| !current_names.contains(filename.as_str()))
    {
        let path = match contained_storage_target(&job.path, &filename).await {
            Ok(path) => path,
            Err(error) => {
                warn!(filename, %error, "Failed to resolve replaced video thumbnail");
                continue;
            }
        };
        if let Err(error) = fs::remove_file(&path).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            warn!(path = %path.display(), %error, "Failed to remove replaced video thumbnail");
        }
    }
    Ok(())
}

async fn encode_variant(
    source: &Path,
    output: &Path,
    profile: &VideoProfile,
    control: ProcessingControl,
) -> Result<(), String> {
    run_ffmpeg(variant_command_args(source, output, profile), control).await
}

fn variant_command_args(source: &Path, output: &Path, profile: &VideoProfile) -> Vec<String> {
    let has_explicit_scale = profile.cmd.iter().any(|arg| arg.flag == "-vf");
    let cmd_args = profile_command_args(&profile.cmd);
    let auto_scale = if has_explicit_scale {
        Vec::new()
    } else {
        vec!["-vf".into(), format!("scale=-2:{}", profile.height)]
    };
    [
        vec!["-i".into(), source.to_string_lossy().to_string()],
        vec!["-map".into(), "0:v:0".into(), "-map".into(), "0:a?".into()],
        cmd_args,
        auto_scale,
        vec!["-threads".into(), VIDEO_PROCESSING_THREADS.to_string()],
        vec![output.to_string_lossy().to_string()],
    ]
    .concat()
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

fn codec_from_cmd_any(cmd: &[VideoProfileArg], flags: &[&str]) -> Option<String> {
    cmd.iter()
        .find(|argument| flags.contains(&argument.flag.as_str()))
        .map(|argument| argument.value.clone())
}

async fn create_thumbnail(
    source: &Path,
    output: &Path,
    width: u32,
    duration_ms: i64,
    control: ProcessingControl,
) -> Result<(), String> {
    let seek_ms = (duration_ms / 10).clamp(0, 1_000);
    create_thumbnail_at(source, output, width, seek_ms, control).await
}

async fn create_thumbnail_at(
    source: &Path,
    output: &Path,
    width: u32,
    seek_ms: i64,
    control: ProcessingControl,
) -> Result<(), String> {
    run_ffmpeg(
        vec![
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
        ],
        control,
    )
    .await
}

async fn run_ffmpeg(args: Vec<String>, control: ProcessingControl) -> Result<(), String> {
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
    let output = tokio::select! {
        output = time::timeout(
            Duration::from_secs(*VIDEO_PROCESSING_TIMEOUT_SECONDS),
            command.output(),
        ) => output
            .map_err(|_| "ffmpeg timed out.".to_string())?
            .map_err(|error| format!("Failed to start ffmpeg: {error}"))?,
        _ = control.cancelled() => return Err(PROCESSING_CANCELLED.into()),
    };
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
            "stream=codec_type,codec_name,width,height,duration:format=format_name,duration",
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
    let stream_duration_ms = stream.duration.as_deref().and_then(parse_duration_ms);
    let duration_ms = probe
        .format
        .as_ref()
        .and_then(|format| format.duration.as_deref())
        .and_then(parse_duration_ms)
        .or(stream_duration_ms);
    let format_name = probe.format.and_then(|format| format.format_name);
    let video_codec = stream.codec_name.clone();
    let audio_codec = probe
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("audio"))
        .and_then(|stream| stream.codec_name.clone());
    Ok(VideoInfo {
        width,
        height,
        duration_ms,
        format_name,
        video_codec,
        audio_codec,
    })
}

fn validate_source(info: &VideoInfo, mime_type: &str) -> Result<(), String> {
    if info.width == 0 || info.height == 0 {
        return Err("Video dimensions must be positive.".into());
    }
    let pixels = u64::from(info.width) * u64::from(info.height);
    if pixels > *VIDEO_PROCESSING_MAX_PIXELS {
        return Err(format!(
            "Video exceeds the configured {} pixel limit.",
            *VIDEO_PROCESSING_MAX_PIXELS
        ));
    }
    let duration_ms = info
        .duration_ms
        .filter(|duration| *duration > 0)
        .ok_or_else(|| "Video duration is missing or invalid.".to_string())?;
    let max_duration_ms = i64::try_from(*VIDEO_PROCESSING_MAX_DURATION_SECONDS)
        .unwrap_or(i64::MAX / 1_000)
        .saturating_mul(1_000);
    if duration_ms > max_duration_ms {
        return Err(format!(
            "Video exceeds the configured {} second duration limit.",
            *VIDEO_PROCESSING_MAX_DURATION_SECONDS
        ));
    }
    if !format_matches_mime(info.format_name.as_deref(), mime_type) {
        return Err("Video content does not match its filename extension.".into());
    }
    Ok(())
}

fn format_matches_mime(format_name: Option<&str>, mime_type: &str) -> bool {
    let Some(format_name) = format_name else {
        return false;
    };
    let formats: HashSet<&str> = format_name.split(',').collect();
    match mime_type {
        "video/mp4" | "video/quicktime" => formats.contains("mov") || formats.contains("mp4"),
        "video/webm" => formats.contains("webm") || formats.contains("matroska"),
        "video/ogg" => formats.contains("ogg"),
        _ => false,
    }
}

fn format_matches_container(format_name: Option<&str>, container: &str) -> bool {
    let Some(format_name) = format_name else {
        return false;
    };
    let formats: HashSet<&str> = format_name.split(',').collect();
    match container.to_ascii_lowercase().as_str() {
        "mp4" | "m4v" | "mov" => formats.contains("mov") || formats.contains("mp4"),
        "webm" => formats.contains("webm") || formats.contains("matroska"),
        "mkv" | "matroska" => formats.contains("matroska"),
        "ogg" | "ogv" => formats.contains("ogg"),
        "avi" => formats.contains("avi"),
        "ts" | "mpegts" => formats.contains("mpegts"),
        _ => false,
    }
}

fn parse_duration_ms(duration: &str) -> Option<i64> {
    duration
        .parse::<f64>()
        .ok()
        .filter(|duration| duration.is_finite() && *duration >= 0.0)
        .and_then(|duration| i64::try_from((duration * 1_000.0).round() as i128).ok())
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

async fn validate_available_encoders(profiles: &[VideoProfile]) -> Result<(), String> {
    let available = AVAILABLE_ENCODERS
        .get_or_try_init(detect_available_encoders)
        .await?;
    for profile in profiles {
        for flag in ["-c:v", "-codec:v", "-vcodec", "-c:a", "-codec:a", "-acodec"] {
            let Some(encoder) = codec_from_cmd(&profile.cmd, flag) else {
                continue;
            };
            if encoder != "copy" && !available.contains(&encoder) {
                return Err(format!(
                    "Video profile '{}' requires unavailable ffmpeg encoder '{}'.",
                    profile.name, encoder
                ));
            }
        }
    }
    Ok(())
}

async fn detect_available_encoders() -> Result<HashSet<String>, String> {
    let mut command = Command::new(ffmpeg_bin());
    command
        .args(["-hide_banner", "-encoders"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = time::timeout(PROBE_TIMEOUT, command.output())
        .await
        .map_err(|_| "ffmpeg encoder detection timed out.".to_string())?
        .map_err(|error| format!("Failed to start ffmpeg encoder detection: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "ffmpeg encoder detection exited with {}: {}",
            output.status,
            truncate_error(&String::from_utf8_lossy(&output.stderr))
        ));
    }
    let encoders = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut columns = line.split_whitespace();
            let flags = columns.next()?;
            let name = columns.next()?;
            (flags.len() == 6 && flags != "------").then(|| name.to_string())
        })
        .collect::<HashSet<_>>();
    if encoders.is_empty() {
        return Err("ffmpeg did not report any available encoders.".into());
    }
    Ok(encoders)
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
        || profile.name != profile.name.to_ascii_lowercase()
        || profile.container != profile.container.to_ascii_lowercase()
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
        MANUAL_THUMBNAIL_JOB_KIND, ProcessedVariant, PublishedFile, RANDOM_THUMBNAIL_JOB_KIND,
        VideoInfo, VideoProfile, claim_job, enqueue_video_processing, enqueue_video_thumbnail,
        fail_job, media_status_after_failure, persist_outputs, profile_command_args,
        publish_staged_file, random_thumbnail_seek, remove_published_outputs, select_profiles,
        validate_source, validate_variant, validate_video_profile, variant_command_args,
        variant_filename,
    };

    const MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

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

    fn sample_video_info() -> VideoInfo {
        VideoInfo {
            width: 1_920,
            height: 1_080,
            duration_ms: Some(60_000),
            format_name: Some("mov,mp4,m4a,3gp,3g2,mj2".into()),
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
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
            format_name: Some("mov,mp4".into()),
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
        };
        assert_eq!(info.width * info.height, 2_073_600);
    }

    #[test]
    fn validates_source_container_and_required_metadata() {
        let info = sample_video_info();
        assert!(validate_source(&info, "video/mp4").is_ok());
        assert!(validate_source(&info, "video/webm").is_err());

        let mut missing_duration = info;
        missing_duration.duration_ms = None;
        assert!(validate_source(&missing_duration, "video/mp4").is_err());
    }

    #[test]
    fn enforces_source_pixel_and_duration_limits_at_the_boundary() {
        let max_pixels = u32::try_from(*super::VIDEO_PROCESSING_MAX_PIXELS)
            .expect("configured maximum pixels fit in u32");
        let max_duration_ms = i64::try_from(*super::VIDEO_PROCESSING_MAX_DURATION_SECONDS)
            .expect("configured maximum duration fits in i64")
            * 1_000;
        let mut info = sample_video_info();
        info.width = max_pixels;
        info.height = 1;
        info.duration_ms = Some(max_duration_ms);
        assert!(validate_source(&info, "video/mp4").is_ok());

        info.width = max_pixels + 1;
        assert!(validate_source(&info, "video/mp4").is_err());
        info.width = 1;
        info.duration_ms = Some(max_duration_ms + 1);
        assert!(validate_source(&info, "video/mp4").is_err());
        info.duration_ms = Some(1);
        info.width = 0;
        assert!(validate_source(&info, "video/mp4").is_err());
    }

    #[test]
    fn validates_generated_variant_dimensions_duration_container_and_codecs() {
        let source = sample_video_info();
        let mut profile = sample_profile();
        profile.cmd.extend([
            VideoProfileArg {
                flag: "-c:a".into(),
                value: "aac".into(),
            },
            VideoProfileArg {
                flag: "-pix_fmt".into(),
                value: "yuv420p".into(),
            },
        ]);
        let mut variant = VideoInfo {
            width: 854,
            height: 480,
            duration_ms: Some(60_000),
            format_name: Some("mov,mp4".into()),
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
        };
        assert!(validate_variant(&source, &variant, &profile).is_ok());

        variant.width = 853;
        assert!(validate_variant(&source, &variant, &profile).is_err());
        variant.width = 854;
        variant.duration_ms = Some(50_000);
        assert!(validate_variant(&source, &variant, &profile).is_err());
        variant.duration_ms = Some(60_000);
        variant.format_name = Some("webm".into());
        assert!(validate_variant(&source, &variant, &profile).is_err());
        variant.format_name = Some("mov,mp4".into());
        variant.video_codec = Some("vp9".into());
        assert!(validate_variant(&source, &variant, &profile).is_err());
        variant.video_codec = Some("h264".into());
        variant.audio_codec = None;
        assert!(validate_variant(&source, &variant, &profile).is_err());
        variant.audio_codec = Some("aac".into());
        variant.height = 1_200;
        assert!(validate_variant(&source, &variant, &profile).is_err());
    }

    #[test]
    fn final_variant_names_do_not_contain_job_or_attempt_ids() {
        assert_eq!(
            variant_filename("sermon", &sample_profile()),
            "sermon--h264-480.mp4"
        );
    }

    #[test]
    fn selects_only_profiles_that_do_not_upscale() {
        let mut hd = sample_profile();
        hd.name = "h264-1080".into();
        hd.height = 1_080;

        let selected = select_profiles(vec![sample_profile(), hd], 720).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].height, 480);
    }

    #[test]
    fn small_source_fallback_keeps_the_bounded_profile_name() {
        let mut profile = sample_profile();
        profile.name = "a".repeat(64);

        let selected = select_profiles(vec![profile], 240).expect("fallback profile is selected");

        assert_eq!(selected[0].name.len(), 64);
        assert_eq!(selected[0].height, 240);
    }

    #[test]
    fn generated_variant_arguments_preserve_profile_order_and_add_safe_scale() {
        let profile = sample_profile();
        let args = variant_command_args(
            std::path::Path::new("source.mp4"),
            std::path::Path::new("output.mp4"),
            &profile,
        );

        assert_eq!(
            args,
            vec![
                "-i".to_string(),
                "source.mp4".to_string(),
                "-map".to_string(),
                "0:v:0".to_string(),
                "-map".to_string(),
                "0:a?".to_string(),
                "-c:v".to_string(),
                "libx264".to_string(),
                "-vf".to_string(),
                "scale=-2:480".to_string(),
                "-threads".to_string(),
                super::VIDEO_PROCESSING_THREADS.to_string(),
                "output.mp4".to_string(),
            ]
        );
    }

    #[test]
    fn retries_only_transient_processing_failures() {
        assert!(super::is_retryable_processing_error("ffmpeg timed out."));
        assert!(super::is_retryable_processing_error(
            "Connection reset by peer"
        ));
        assert!(!super::is_retryable_processing_error(
            "No video stream found."
        ));
        assert!(!super::is_retryable_processing_error(
            "ffmpeg encoder unavailable"
        ));
    }

    #[test]
    fn random_thumbnail_seek_stays_within_the_video_duration() {
        for _ in 0..32 {
            let seek = random_thumbnail_seek(Some(10_000)).expect("duration is valid");
            assert!((500..9_500).contains(&seek));
        }
        assert_eq!(
            random_thumbnail_seek(Some(250)).expect("short duration is valid"),
            0
        );
        assert!(random_thumbnail_seek(None).is_err());
        assert!(random_thumbnail_seek(Some(0)).is_err());
    }

    #[test]
    fn failed_thumbnail_does_not_mark_a_processed_video_as_failed() {
        assert_eq!(
            media_status_after_failure(MANUAL_THUMBNAIL_JOB_KIND, true),
            "completed"
        );
        assert_eq!(
            media_status_after_failure(RANDOM_THUMBNAIL_JOB_KIND, false),
            "completed"
        );
        assert_eq!(media_status_after_failure(super::JOB_KIND, true), "failed");
    }

    #[tokio::test]
    async fn replacing_an_output_keeps_a_rollback_backup() {
        let directory =
            std::env::temp_dir().join(format!("nur-cms-video-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&directory)
            .await
            .expect("test directory can be created");
        let staged = directory.join("staged.jpg");
        let target = directory.join("published.jpg");
        tokio::fs::write(&staged, b"new")
            .await
            .expect("staged file can be written");
        tokio::fs::write(&target, b"old")
            .await
            .expect("existing file can be written");

        let published = publish_staged_file(&staged, &target)
            .await
            .expect("replacement can be published");
        assert_eq!(
            tokio::fs::read(&target)
                .await
                .expect("new output remains readable"),
            b"new"
        );
        remove_published_outputs(&[published]).await;
        assert_eq!(
            tokio::fs::read(&target)
                .await
                .expect("old output is restored"),
            b"old"
        );

        tokio::fs::remove_dir_all(directory)
            .await
            .expect("test directory can be removed");
    }

    #[tokio::test]
    async fn published_outputs_can_be_rolled_back() {
        let directory =
            std::env::temp_dir().join(format!("nur-cms-video-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&directory)
            .await
            .expect("test directory can be created");
        let first = directory.join("first.jpg");
        let second = directory.join("second.jpg");
        tokio::fs::write(&first, b"first")
            .await
            .expect("first output can be written");
        tokio::fs::write(&second, b"second")
            .await
            .expect("second output can be written");

        remove_published_outputs(&[
            PublishedFile {
                target: first.clone(),
                backup: None,
            },
            PublishedFile {
                target: second.clone(),
                backup: None,
            },
        ])
        .await;

        assert!(
            !tokio::fs::try_exists(first)
                .await
                .expect("first output can be checked")
        );
        assert!(
            !tokio::fs::try_exists(second)
                .await
                .expect("second output can be checked")
        );
        tokio::fs::remove_dir_all(directory)
            .await
            .expect("test directory can be removed");
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

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn a_second_worker_cannot_claim_an_active_job(pool: PgPool) {
        let media_id: i32 = sqlx::query_scalar(
            "INSERT INTO media (filename, path, type) VALUES ('claim.mp4', '/uploads', 'video/mp4') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("video can be inserted");
        enqueue_video_processing(&pool, media_id)
            .await
            .expect("video processing can be queued");

        assert!(
            claim_job(&pool)
                .await
                .expect("first claim succeeds")
                .is_some()
        );
        assert!(
            claim_job(&pool)
                .await
                .expect("second claim succeeds")
                .is_none()
        );
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn an_expired_lease_can_be_reclaimed(pool: PgPool) {
        let media_id: i32 = sqlx::query_scalar(
            "INSERT INTO media (filename, path, type) VALUES ('expired.mp4', '/uploads', 'video/mp4') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("video can be inserted");
        enqueue_video_processing(&pool, media_id)
            .await
            .expect("video processing can be queued");
        let first = claim_job(&pool)
            .await
            .expect("first claim succeeds")
            .expect("job is available");
        sqlx::query(
            "UPDATE media_processing_jobs SET lease_expires_at = now() - interval '1 second', updated_at = now() - interval '5 minutes' WHERE id = $1",
        )
        .bind(first.id)
        .execute(&pool)
        .await
        .expect("lease can be expired");

        let reclaimed = claim_job(&pool)
            .await
            .expect("reclaim succeeds")
            .expect("expired job is available");
        assert_eq!(reclaimed.id, first.id);
        assert_ne!(reclaimed.lease_token, first.lease_token);
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn permanent_failures_stop_but_transient_failures_are_requeued(pool: PgPool) {
        for (filename, retryable, expected_status) in [
            ("invalid.mp4", false, "failed"),
            ("temporary.mp4", true, "queued"),
        ] {
            let media_id: i32 = sqlx::query_scalar(
                "INSERT INTO media (filename, path, type) VALUES ($1, '/uploads', 'video/mp4') RETURNING id",
            )
            .bind(filename)
            .fetch_one(&pool)
            .await
            .expect("video can be inserted");
            enqueue_video_processing(&pool, media_id)
                .await
                .expect("video processing can be queued");
            let job = claim_job(&pool)
                .await
                .expect("claim succeeds")
                .expect("job is available");

            fail_job(&pool, &job, "test failure", retryable)
                .await
                .expect("failure can be stored");
            let status: String =
                sqlx::query_scalar("SELECT status FROM media_processing_jobs WHERE id = $1")
                    .bind(job.id)
                    .fetch_one(&pool)
                    .await
                    .expect("status can be read");
            assert_eq!(status, expected_status);
        }
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn retry_limit_is_enforced_and_failed_jobs_can_be_explicitly_requeued(pool: PgPool) {
        let media_id: i32 = sqlx::query_scalar(
            "INSERT INTO media (filename, path, type) VALUES ('retry.mp4', '/uploads', 'video/mp4') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("video can be inserted");
        enqueue_video_processing(&pool, media_id)
            .await
            .expect("video processing can be queued");
        sqlx::query("UPDATE media_processing_jobs SET max_attempts = 1 WHERE media_id = $1")
            .bind(media_id)
            .execute(&pool)
            .await
            .expect("retry limit can be adjusted");
        let job = claim_job(&pool)
            .await
            .expect("claim succeeds")
            .expect("job is available");
        fail_job(&pool, &job, "temporary failure", true)
            .await
            .expect("failure can be stored");

        let status: String =
            sqlx::query_scalar("SELECT status FROM media_processing_jobs WHERE id = $1")
                .bind(job.id)
                .fetch_one(&pool)
                .await
                .expect("status can be read");
        assert_eq!(status, "failed");

        enqueue_video_processing(&pool, media_id)
            .await
            .expect("explicit retry creates a new job");
        let active_jobs: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM media_processing_jobs WHERE media_id = $1 AND status = 'queued'",
        )
        .bind(media_id)
        .fetch_one(&pool)
        .await
        .expect("active job can be counted");
        assert_eq!(active_jobs, 1);
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn upload_recovery_does_not_duplicate_an_existing_video_job(pool: PgPool) {
        let media_id: i32 = sqlx::query_scalar(
            "INSERT INTO media (filename, path, type, processing_status) VALUES ('recovery.mp4', '/uploads', 'video/mp4', 'queued') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("video can be inserted");

        super::ensure_video_processing(&pool, media_id)
            .await
            .expect("missing job can be recovered");
        super::ensure_video_processing(&pool, media_id)
            .await
            .expect("recovery is idempotent");
        let jobs: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM media_processing_jobs WHERE media_id = $1 AND kind = $2",
        )
        .bind(media_id)
        .bind(super::JOB_KIND)
        .fetch_one(&pool)
        .await
        .expect("jobs can be counted");
        assert_eq!(jobs, 1);
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn losing_the_lease_rolls_back_all_variant_rows(pool: PgPool) {
        let media_id: i32 = sqlx::query_scalar(
            "INSERT INTO media (filename, path, type) VALUES ('lease.mp4', '/uploads', 'video/mp4') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("video can be inserted");
        enqueue_video_processing(&pool, media_id)
            .await
            .expect("video processing can be queued");
        let job = claim_job(&pool)
            .await
            .expect("claim succeeds")
            .expect("job is available");
        sqlx::query("UPDATE media_processing_jobs SET lease_token = 'new-owner' WHERE id = $1")
            .bind(job.id)
            .execute(&pool)
            .await
            .expect("lease ownership can be changed");

        let variant = ProcessedVariant {
            profile: sample_profile(),
            filename: "lease--h264-480.mp4".into(),
            staging_path: std::path::PathBuf::from("unused-in-persistence-test"),
            width: 854,
            height: 480,
            size: 1_024,
            duration_ms: Some(60_000),
        };
        let transaction = pool.begin().await.expect("transaction can start");
        assert!(
            persist_outputs(transaction, &job, &[variant], &[])
                .await
                .is_err()
        );

        let variants: i64 =
            sqlx::query_scalar("SELECT count(*) FROM media_video_variants WHERE media_id = $1")
                .bind(media_id)
                .fetch_one(&pool)
                .await
                .expect("variant rows can be counted");
        let media_status: Option<String> =
            sqlx::query_scalar("SELECT processing_status FROM media WHERE id = $1")
                .bind(media_id)
                .fetch_one(&pool)
                .await
                .expect("media status can be read");
        assert_eq!(variants, 0);
        assert_eq!(media_status.as_deref(), Some("processing"));
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn thumbnail_jobs_do_not_run_alongside_video_processing(pool: PgPool) {
        let media_id: i32 = sqlx::query_scalar(
            "INSERT INTO media (filename, path, type) VALUES ('video.mp4', '/uploads', 'video/mp4') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("video can be inserted");
        enqueue_video_processing(&pool, media_id)
            .await
            .expect("video processing can be queued");

        let thumbnail = enqueue_video_thumbnail(&pool, media_id, None).await;
        assert!(matches!(
            thumbnail,
            Err(crate::utils::errors::NurError::Conflict(_))
        ));
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn manual_and_random_thumbnail_jobs_remain_distinguishable(pool: PgPool) {
        let source_id: i32 = sqlx::query_scalar(
            "INSERT INTO media (filename, path, type) VALUES ('poster.jpg', '/uploads', 'image/jpeg') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("source image can be inserted");
        let manual_video_id: i32 = sqlx::query_scalar(
            "INSERT INTO media (filename, path, type) VALUES ('manual.mp4', '/uploads', 'video/mp4') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("manual video can be inserted");
        let random_video_id: i32 = sqlx::query_scalar(
            "INSERT INTO media (filename, path, type) VALUES ('random.mp4', '/uploads', 'video/mp4') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("random video can be inserted");

        enqueue_video_thumbnail(&pool, manual_video_id, Some(source_id))
            .await
            .expect("manual thumbnail can be queued");
        enqueue_video_thumbnail(&pool, random_video_id, None)
            .await
            .expect("random thumbnail can be queued");
        let processing_statuses: Vec<String> = sqlx::query_scalar(
            "SELECT processing_status FROM media WHERE id IN ($1, $2) ORDER BY id",
        )
        .bind(manual_video_id)
        .bind(random_video_id)
        .fetch_all(&pool)
        .await
        .expect("video processing statuses can be queried");
        assert_eq!(processing_statuses, ["completed", "completed"]);
        sqlx::query("DELETE FROM media WHERE id = $1")
            .bind(source_id)
            .execute(&pool)
            .await
            .expect("source image can be deleted");

        let jobs: Vec<(String, Option<i32>)> = sqlx::query_as(
            "SELECT kind, source_media_id FROM media_processing_jobs ORDER BY media_id",
        )
        .fetch_all(&pool)
        .await
        .expect("thumbnail jobs can be queried");
        assert_eq!(
            jobs,
            vec![
                (MANUAL_THUMBNAIL_JOB_KIND.into(), None),
                (RANDOM_THUMBNAIL_JOB_KIND.into(), None),
            ]
        );
    }
}
