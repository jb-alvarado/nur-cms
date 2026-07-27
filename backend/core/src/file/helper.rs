use std::{
    collections::HashMap,
    ops::Range,
    path::{Component, Path, PathBuf},
    sync::{Arc, LazyLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use tokio::{
    fs,
    sync::{Mutex, broadcast::Sender},
};
use tracing::{error, info};

use crate::{
    IMAGE_PROCESSING_SEMAPHORE, MAX_ACTIVE_UPLOADS_PER_USER, MAX_IMAGE_PIXELS, PUBLIC_UPLOADS,
    STORAGE, UPLOAD_TTL_SECONDS,
    db::{models::Configuration, serialize::MediaSerializer},
    file::processing::save_image,
    sse::{SSELevel as Level, SSEMessage},
    utils::errors::NurError,
};

#[derive(Debug, Serialize, Deserialize)]
struct PersistedUpload {
    user_id: i32,
    total_size: u64,
    ranges: Vec<(u64, u64)>,
    #[serde(default)]
    updated_at: u64,
}

#[derive(Debug)]
struct UploadState {
    batch_id: String,
    user_id: i32,
    total_size: u64,
    ranges: Vec<Range<u64>>,
    finalizing: bool,
    updated_at: SystemTime,
}

#[derive(Clone)]
pub struct Upload {
    state: Arc<Mutex<UploadState>>,
    pub temp_file: PathBuf,
    pub metadata_file: PathBuf,
}

/// Tracks byte ranges for resumable uploads
pub type UploadMap = HashMap<String, Upload>;

/// Global upload map, protected by a Mutex
pub static UPLOADS: LazyLock<Mutex<UploadMap>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn unix_timestamp(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub async fn cleanup_stale_uploads() {
    let now = SystemTime::now();
    let ttl = Duration::from_secs(*UPLOAD_TTL_SECONDS);
    let mut uploads = UPLOADS.lock().await;
    let mut expired = Vec::new();

    for (key, upload) in uploads.iter() {
        let state = upload.state.lock().await;
        if !state.finalizing && now.duration_since(state.updated_at).unwrap_or_default() >= ttl {
            expired.push((
                key.clone(),
                upload.temp_file.clone(),
                upload.metadata_file.clone(),
            ));
        }
    }

    for (key, _, _) in &expired {
        uploads.remove(key);
    }
    drop(uploads);

    for (_, temp_file, metadata_file) in expired {
        remove_upload_files(&temp_file, &metadata_file).await;
    }
}

async fn remove_upload_files(temp_file: &Path, metadata_file: &Path) {
    for path in [temp_file, metadata_file] {
        if let Err(error) = fs::remove_file(path).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            error!("Failed to remove stale upload {}: {error}", path.display());
        }
    }
}

/// Remove persisted uploads that cannot be resumed after a process restart.
///
/// A metadata file without its temporary data file is always orphaned. Complete
/// pairs are retained for the configured TTL so interrupted uploads remain
/// resumable.
pub async fn cleanup_persisted_uploads() {
    let root = PathBuf::from(STORAGE.as_str());
    let now = SystemTime::now();
    let ttl = Duration::from_secs(*UPLOAD_TTL_SECONDS);
    let mut directories = vec![root];

    while let Some(directory) = directories.pop() {
        let mut entries = match fs::read_dir(&directory).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                error!(
                    "Failed to read upload directory {}: {error}",
                    directory.display()
                );
                continue;
            }
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let file_type = match entry.file_type().await {
                Ok(file_type) => file_type,
                Err(error) => {
                    error!("Failed to inspect upload path {}: {error}", path.display());
                    continue;
                }
            };

            if file_type.is_dir() {
                directories.push(path);
                continue;
            }
            if !file_type.is_file()
                || !path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".uploading.json"))
            {
                continue;
            }

            let Some(temp_file) = path.to_str().and_then(|name| name.strip_suffix(".json")) else {
                continue;
            };
            let temp_file = PathBuf::from(temp_file);

            if !fs::try_exists(&temp_file).await.unwrap_or(false) {
                remove_upload_files(&temp_file, &path).await;
                continue;
            }

            let updated_at = match fs::read(&path).await {
                Ok(data) => serde_json::from_slice::<PersistedUpload>(&data)
                    .ok()
                    .map(|upload| UNIX_EPOCH + Duration::from_secs(upload.updated_at)),
                Err(_) => None,
            };
            let updated_at = match updated_at {
                Some(updated_at) => Some(updated_at),
                None => fs::metadata(&path)
                    .await
                    .ok()
                    .and_then(|metadata| metadata.modified().ok()),
            };

            if updated_at
                .is_some_and(|updated_at| now.duration_since(updated_at).unwrap_or_default() >= ttl)
            {
                remove_upload_files(&temp_file, &path).await;
            }
        }
    }
}

fn safe_file_name(file_name: &str) -> Result<&str, NurError> {
    let mut components = Path::new(file_name).components();
    let is_single_normal_component =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();

    if file_name.is_empty()
        || !is_single_normal_component
        || sanitize_filename::sanitize(file_name) != file_name
    {
        return Err(NurError::BadRequest("Invalid filename.".into()));
    }

    Ok(file_name)
}

fn storage_relative_path(public_path: &str) -> Result<PathBuf, NurError> {
    let path = Path::new(public_path);
    let relative = path.strip_prefix(PUBLIC_UPLOADS).unwrap_or(path);

    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(NurError::BadRequest("Invalid media path.".into()));
    }

    Ok(relative.to_path_buf())
}

async fn contained_storage_target(public_path: &str, file_name: &str) -> Result<PathBuf, NurError> {
    let file_name = safe_file_name(file_name)?;
    let storage_root = fs::canonicalize(STORAGE.as_str()).await?;
    let relative = storage_relative_path(public_path)?;
    let parent = fs::canonicalize(storage_root.join(relative)).await?;

    if !parent.starts_with(&storage_root) {
        return Err(NurError::Forbidden(
            "Media path escapes the storage directory.".into(),
        ));
    }

    Ok(parent.join(file_name))
}

/// Merge overlapping or adjacent ranges
pub fn merge_ranges(ranges: &mut Vec<Range<u64>>) {
    if ranges.is_empty() {
        return;
    }

    ranges.sort_by_key(|r| r.start);
    let mut merged = vec![ranges[0].clone()];

    for r in ranges.iter().skip(1) {
        let last = merged.last_mut().unwrap();
        if last.end >= r.start {
            last.end = last.end.max(r.end); // merge overlapping or adjacent ranges
        } else {
            merged.push(r.clone());
        }
    }

    *ranges = merged;
}

/// Check if upload is complete
pub fn is_upload_complete(ranges: &[Range<u64>], total_size: u64) -> bool {
    if ranges.is_empty() {
        return false;
    }

    let mut pos = 0;
    for r in ranges {
        if r.start != pos {
            return false; // gap detected
        }
        pos = r.end;
    }

    pos == total_size
}

fn append_extension(path: &Path, extension: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(extension);
    PathBuf::from(name)
}

pub fn uploading_path(output_file: &Path) -> PathBuf {
    append_extension(output_file, ".uploading")
}

fn metadata_path(temp_file: &Path) -> PathBuf {
    append_extension(temp_file, ".json")
}

pub async fn get_active_upload(
    output_file: &Path,
    batch_id: &str,
    user_id: i32,
    total_size: u64,
) -> Result<Option<Upload>, NurError> {
    let upload_key = output_file.to_string_lossy().to_string();
    let uploads = UPLOADS.lock().await;
    let Some(upload) = uploads.get(&upload_key) else {
        return Ok(None);
    };

    let mut state = upload.state.lock().await;
    if state.batch_id != batch_id {
        if state.ranges.is_empty() && !state.finalizing {
            state.batch_id = batch_id.to_string();
        } else {
            return Err(NurError::Conflict(
                "Another upload is already writing this file.".into(),
            ));
        }
    }
    if state.user_id != user_id || state.total_size != total_size {
        return Err(NurError::Conflict(
            "Upload metadata does not match the existing upload.".into(),
        ));
    }
    state.updated_at = SystemTime::now();
    drop(state);

    Ok(Some(upload.clone()))
}

async fn persist_upload(upload: &Upload, state: &UploadState) -> Result<(), NurError> {
    let persisted = PersistedUpload {
        user_id: state.user_id,
        total_size: state.total_size,
        ranges: state.ranges.iter().map(|r| (r.start, r.end)).collect(),
        updated_at: unix_timestamp(state.updated_at),
    };
    let data = serde_json::to_vec(&persisted)?;
    let temporary_metadata = append_extension(&upload.metadata_file, ".tmp");

    fs::write(&temporary_metadata, data).await?;
    fs::rename(&temporary_metadata, &upload.metadata_file).await?;

    Ok(())
}

/// Get or restore the tracked state for a file upload.
pub async fn get_or_create_upload(
    total_size: u64,
    output_file: &Path,
    batch_id: &str,
    user_id: i32,
) -> Result<Upload, NurError> {
    let upload_key = output_file.to_string_lossy().to_string();
    let mut uploads = UPLOADS.lock().await;

    if let Some(upload) = uploads.get(&upload_key) {
        let mut state = upload.state.lock().await;
        if state.batch_id != batch_id {
            if state.ranges.is_empty() && !state.finalizing {
                state.batch_id = batch_id.to_string();
            } else {
                return Err(NurError::Conflict(
                    "Another upload is already writing this file.".into(),
                ));
            }
        }
        if state.user_id != user_id || state.total_size != total_size {
            return Err(NurError::Conflict(
                "Upload metadata does not match the existing upload.".into(),
            ));
        }
        state.updated_at = SystemTime::now();
        drop(state);
        return Ok(upload.clone());
    }

    let mut active_for_user = 0usize;
    for upload in uploads.values() {
        if upload.state.lock().await.user_id == user_id {
            active_for_user += 1;
        }
    }
    if active_for_user >= *MAX_ACTIVE_UPLOADS_PER_USER {
        return Err(NurError::ToManyRequests);
    }

    if fs::try_exists(output_file).await? {
        return Err(NurError::Conflict(format!(
            "File '{}' already exists on disk.",
            output_file.display()
        )));
    }

    let temp_file = uploading_path(output_file);
    let metadata_file = metadata_path(&temp_file);
    let state = if fs::try_exists(&metadata_file).await? {
        let data = fs::read(&metadata_file).await?;
        let persisted: PersistedUpload = serde_json::from_slice(&data)?;

        if persisted.user_id != user_id || persisted.total_size != total_size {
            return Err(NurError::Conflict(
                "An incompatible incomplete upload already exists.".into(),
            ));
        }

        let mut ranges = persisted
            .ranges
            .into_iter()
            .map(|(start, end)| start..end)
            .collect::<Vec<_>>();

        if ranges
            .iter()
            .any(|range| range.start >= range.end || range.end > total_size)
        {
            return Err(NurError::Conflict(
                "Stored upload ranges are invalid.".into(),
            ));
        }

        if let Some(last_end) = ranges.iter().map(|range| range.end).max() {
            let temp_size = fs::metadata(&temp_file).await.map(|meta| meta.len());
            if temp_size.is_err() || temp_size.is_ok_and(|size| size < last_end) {
                return Err(NurError::Conflict(
                    "Incomplete upload data does not match its resume metadata.".into(),
                ));
            }
        }

        merge_ranges(&mut ranges);

        let updated_at = if persisted.updated_at == 0 {
            fs::metadata(&metadata_file)
                .await
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .unwrap_or_else(SystemTime::now)
        } else {
            UNIX_EPOCH + Duration::from_secs(persisted.updated_at)
        };
        if SystemTime::now()
            .duration_since(updated_at)
            .unwrap_or_default()
            >= Duration::from_secs(*UPLOAD_TTL_SECONDS)
        {
            drop(uploads);
            for path in [&temp_file, &metadata_file] {
                if let Err(error) = fs::remove_file(path).await
                    && error.kind() != std::io::ErrorKind::NotFound
                {
                    return Err(error.into());
                }
            }
            return Box::pin(get_or_create_upload(
                total_size,
                output_file,
                batch_id,
                user_id,
            ))
            .await;
        }

        UploadState {
            batch_id: batch_id.to_string(),
            user_id,
            total_size,
            ranges,
            finalizing: false,
            updated_at,
        }
    } else {
        if fs::try_exists(&temp_file).await? {
            return Err(NurError::Conflict(format!(
                "Incomplete upload '{}' has no resume metadata.",
                temp_file.display()
            )));
        }

        UploadState {
            batch_id: batch_id.to_string(),
            user_id,
            total_size,
            ranges: Vec::new(),
            finalizing: false,
            updated_at: SystemTime::now(),
        }
    };

    let upload = Upload {
        state: Arc::new(Mutex::new(state)),
        temp_file,
        metadata_file,
    };

    {
        let state = upload.state.lock().await;
        persist_upload(&upload, &state).await?;
    }

    uploads.insert(upload_key, upload.clone());
    info!("Start or resume uploading: {output_file:?}");

    Ok(upload)
}

pub async fn write_upload_chunk(
    upload: &Upload,
    start: u64,
    end: u64,
    chunk_data: &[u8],
) -> Result<bool, NurError> {
    use tokio::io::{AsyncSeekExt, AsyncWriteExt, SeekFrom};

    let mut state = upload.state.lock().await;

    if state.finalizing {
        return Ok(false);
    }
    state.updated_at = SystemTime::now();

    let already_written = state
        .ranges
        .iter()
        .any(|range| range.start <= start && range.end >= end);

    if !already_written {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&upload.temp_file)
            .await?;
        file.seek(SeekFrom::Start(start)).await?;
        file.write_all(chunk_data).await?;
        file.flush().await?;
        file.sync_data().await?;

        state.ranges.push(start..end);
        merge_ranges(&mut state.ranges);
        persist_upload(upload, &state).await?;
    }

    if is_upload_complete(&state.ranges, state.total_size) {
        state.finalizing = true;
        return Ok(true);
    }

    Ok(false)
}

pub async fn reset_finalizing(upload: &Upload) {
    upload.state.lock().await.finalizing = false;
}

pub async fn received_ranges(upload: &Upload) -> Vec<(u64, u64)> {
    let state = upload.state.lock().await;

    // A fully written temporary file still needs one request to claim finalization
    // after a process restart. Returning no ranges makes the client resend a chunk.
    if is_upload_complete(&state.ranges, state.total_size) && !state.finalizing {
        return Vec::new();
    }

    state
        .ranges
        .iter()
        .map(|range| (range.start, range.end))
        .collect()
}

pub async fn cleanup_upload(output_file: &Path, upload: &Upload) {
    let upload_key = output_file.to_string_lossy().to_string();
    UPLOADS.lock().await.remove(&upload_key);

    if let Err(error) = fs::remove_file(&upload.metadata_file).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        error!("Failed to remove upload metadata: {error}");
    }
}

pub async fn delete_media_record(pool: &PgPool, media_id: i32) {
    if let Err(error) = sqlx::query("DELETE FROM media WHERE id = $1")
        .bind(media_id)
        .execute(pool)
        .await
    {
        error!("Failed to roll back media record {media_id}: {error}");
    }
}

/// Add one unique media record for the completed temporary file.
pub async fn add_media_record(
    pool: &PgPool,
    user_id: i32,
    upload_id: &str,
    temp_file: &Path,
    output_file: &Path,
) -> Result<(i32, String, bool), NurError> {
    let mime_type = mime_guess::from_path(output_file)
        .first_or_octet_stream()
        .to_string();

    let (width, height) = if matches!(
        mime_type.as_str(),
        "image/avif" | "image/gif" | "image/jpeg" | "image/jpg" | "image/png" | "image/webp"
    ) {
        let image_path = temp_file.to_path_buf();
        let expected_format = image::ImageFormat::from_extension(
            output_file
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or_default(),
        );
        let dimensions = tokio::task::spawn_blocking(move || -> Result<_, NurError> {
            let reader = image::ImageReader::open(image_path)?.with_guessed_format()?;
            if expected_format.is_none() || reader.format() != expected_format {
                return Err(NurError::BadRequest(
                    "File content does not match its image extension.".into(),
                ));
            }
            Ok(reader.into_dimensions()?)
        })
        .await??;
        let pixels = u64::from(dimensions.0) * u64::from(dimensions.1);
        if pixels == 0 || pixels > *MAX_IMAGE_PIXELS {
            return Err(NurError::BadRequest(format!(
                "Image exceeds the maximum of {} pixels.",
                *MAX_IMAGE_PIXELS
            )));
        }
        (
            Some(i32::try_from(dimensions.0).map_err(|_| NurError::InvalidInput)?),
            Some(i32::try_from(dimensions.1).map_err(|_| NurError::InvalidInput)?),
        )
    } else {
        (None, None)
    };

    let size = fs::metadata(temp_file).await.ok().map(|m| m.len() as i64);

    let path = output_file
        .strip_prefix(STORAGE.as_str())
        .unwrap_or(output_file)
        .parent()
        .map(|p| Path::new(PUBLIC_UPLOADS).join(p))
        .ok_or_else(|| NurError::Conflict("Invalid file path".into()))?
        .to_string_lossy()
        .to_string();

    let filename = output_file
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let alt = output_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");

    let media_id = sqlx::query_scalar::<_, i32>(
        r#"INSERT INTO media
               (alt, filename, path, type, width, height, size, uploaded_by, upload_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
           ON CONFLICT (path, filename) DO NOTHING
           RETURNING id"#,
    )
    .bind(alt)
    .bind(filename)
    .bind(path)
    .bind(&mime_type)
    .bind(width)
    .bind(height)
    .bind(size)
    .bind(user_id)
    .bind(upload_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| NurError::Conflict("File already exists in database.".into()))?;

    Ok((media_id, mime_type, width.is_some()))
}

/// Generate all variants and wait until every database row has been inserted.
pub async fn process_variants(
    pool: &PgPool,
    config: &Configuration,
    output_file: &Path,
    media_id: i32,
    mime_type: &str,
    processable_image: bool,
    tx: &Sender<String>,
) -> Result<(), NurError> {
    let resolutions = config.image_resolutions.clone().unwrap_or_default();
    let extensions = config.image_extensions.clone().unwrap_or_default();

    if !mime_type.starts_with("image") || !processable_image || extensions.is_empty() {
        return Ok(());
    }

    let msg = SSEMessage::new(Level::Info, "Create image variants.");
    let _ = tx.send(msg.to_string());

    let output_file = output_file.to_path_buf();
    let tx_clone = tx.clone();
    let _permit = IMAGE_PROCESSING_SEMAPHORE
        .acquire()
        .await
        .map_err(|_| NurError::ServiceUnavailable("Image processor unavailable.".into()))?;
    let variants = tokio::task::spawn_blocking(move || {
        save_image(resolutions, &extensions, &output_file, Some(tx_clone))
            .map_err(|error| error.to_string())
    })
    .await?
    .map_err(NurError::Conflict)?;

    if variants.is_empty() {
        return Err(NurError::Conflict(
            "No image variants were generated.".into(),
        ));
    }

    for (width, height, filename) in variants {
        sqlx::query(
            r#"INSERT INTO media_variants (media_id, width, height, filename)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (media_id, width, height, filename) DO NOTHING"#,
        )
        .bind(media_id)
        .bind(width)
        .bind(height)
        .bind(filename)
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Rename a media file and its variants on disk
pub async fn rename_media_file(
    media: &mut MediaSerializer,
    new_filename: &str,
) -> Result<(), NurError> {
    let filename = media.filename.clone().unwrap_or_default();
    let media_path = media.path.clone().unwrap_or_default();
    let old_path = contained_storage_target(&media_path, &filename).await?;
    let storage_root = fs::canonicalize(STORAGE.as_str()).await?;
    let new_path = contained_storage_target(&media_path, new_filename).await?;
    let mut rename_pairs = vec![(old_path, new_path)];
    let mut renamed_variants = Vec::new();

    let old_stem = Path::new(&filename)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let new_stem = Path::new(new_filename)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();

    for (index, variant) in media.variants.iter().enumerate() {
        if filename.starts_with(&*old_stem) && variant.filename != filename {
            let new_variant_name = variant.filename.replacen(&*old_stem, &new_stem, 1);
            let old_variant_path = contained_storage_target(&media_path, &variant.filename).await?;
            let new_variant_path = contained_storage_target(&media_path, &new_variant_name).await?;
            rename_pairs.push((old_variant_path, new_variant_path));
            renamed_variants.push((index, new_variant_name));
        }
    }

    for (old, new) in &rename_pairs {
        let canonical_old = fs::canonicalize(old)
            .await
            .map_err(|_| NurError::Conflict(format!("Media file not found: {}", old.display())))?;
        if !canonical_old.starts_with(&storage_root) {
            return Err(NurError::Forbidden(
                "Media file escapes the storage directory.".into(),
            ));
        }
        if fs::try_exists(new).await? {
            return Err(NurError::Conflict(format!(
                "File already exists: {}",
                new.display()
            )));
        }
    }

    for (completed, (old, new)) in rename_pairs.iter().enumerate() {
        if let Err(error) = fs::rename(old, new).await {
            for (rollback_old, rollback_new) in rename_pairs[..completed].iter().rev() {
                if let Err(rollback_error) = fs::rename(rollback_new, rollback_old).await {
                    error!(
                        "Failed to roll back media rename {} -> {}: {rollback_error}",
                        rollback_new.display(),
                        rollback_old.display()
                    );
                }
            }
            return Err(error.into());
        }
        info!("Renamed file: {} -> {}", old.display(), new.display());
    }

    for (index, new_name) in renamed_variants {
        media.variants[index].filename = new_name;
    }
    media.filename = Some(new_filename.to_string());

    Ok(())
}

pub async fn delete_media_file(media: &MediaSerializer) -> Result<(), NurError> {
    let fname = media.filename.clone().unwrap_or_default();
    let rel = media.path.clone().unwrap_or_default();
    let target = contained_storage_target(&rel, &fname).await?;

    if !target.exists() {
        return Err(NurError::Conflict(format!("File not found: {:?}", target)));
    }
    let storage_root = fs::canonicalize(STORAGE.as_str()).await?;
    let target = fs::canonicalize(target).await?;
    if !target.starts_with(&storage_root) {
        return Err(NurError::Forbidden(
            "Media file escapes the storage directory.".into(),
        ));
    }

    // Delete variants recorded for this media row.
    for variant in &media.variants {
        if variant.filename == fname {
            continue;
        }
        let variant_path = contained_storage_target(&rel, &variant.filename).await?;
        if let Err(error) = fs::remove_file(&variant_path).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(error.into());
        }
    }

    // Older media records may predate `media_variants` rows. In that case,
    // clean up only files matching the generated `<stem>-<width>.<extension>`
    // variant convention, never arbitrary siblings in the upload directory.
    if media.variants.is_empty() {
        let stem = Path::new(&fname)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or(NurError::InvalidInput)?;
        let parent = target.parent().ok_or(NurError::InvalidInput)?;
        let mut entries = fs::read_dir(parent).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !entry.file_type().await?.is_file()
                || !path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| is_generated_variant_filename(name, stem))
            {
                continue;
            }

            fs::remove_file(&path).await?;
            info!("Removed untracked media variant {path:?}");
        }
    }

    // Delete main file
    fs::remove_file(&target).await?;
    info!("Removed file {:?}", target);

    Ok(())
}

fn is_generated_variant_filename(filename: &str, stem: &str) -> bool {
    let Some(remainder) = filename.strip_prefix(&format!("{stem}-")) else {
        return false;
    };
    let Some((width, extension)) = remainder.rsplit_once('.') else {
        return false;
    };

    !extension.is_empty() && width.parse::<u32>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::{
        is_generated_variant_filename, is_upload_complete, merge_ranges, safe_file_name,
        storage_relative_path, uploading_path,
    };
    use std::{ops::Range, path::Path};

    #[test]
    fn merges_overlapping_and_adjacent_ranges() {
        let mut ranges: Vec<Range<u64>> = vec![10..20, 0..5, 5..12, 30..40];

        merge_ranges(&mut ranges);

        assert_eq!(ranges, vec![0..20, 30..40]);
    }

    #[test]
    fn only_contiguous_ranges_complete_an_upload() {
        assert!(is_upload_complete(&[0..10, 10..20], 20));
        assert!(!is_upload_complete(&[0..10, 12..20], 20));
    }

    #[test]
    fn appends_uploading_to_the_full_filename() {
        assert_eq!(
            uploading_path(Path::new("/uploads/image.jpg")),
            Path::new("/uploads/image.jpg.uploading")
        );
    }

    #[test]
    fn rejects_unsafe_media_names_and_paths() {
        for name in ["", ".", "..", "../image.jpg", "/tmp/image.jpg"] {
            assert!(safe_file_name(name).is_err(), "{name} should be rejected");
        }
        assert!(safe_file_name("safe-image.jpg").is_ok());

        for path in ["/etc", "../uploads", "/uploads/../etc"] {
            assert!(
                storage_relative_path(path).is_err(),
                "{path} should be rejected"
            );
        }
        assert_eq!(
            storage_relative_path("/uploads/2026/07").expect("safe path"),
            Path::new("2026/07")
        );
    }

    #[test]
    fn recognizes_only_generated_variant_filenames() {
        assert!(is_generated_variant_filename("photo-1280.webp", "photo"));
        assert!(is_generated_variant_filename("photo-320.jpg", "photo"));
        assert!(!is_generated_variant_filename(
            "photo-original.jpg",
            "photo"
        ));
        assert!(!is_generated_variant_filename("photo-320", "photo"));
        assert!(!is_generated_variant_filename("other-320.jpg", "photo"));
    }
}
