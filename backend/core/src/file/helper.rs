use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use tokio::{
    fs,
    sync::{Mutex, broadcast::Sender},
};
use tracing::{error, info};

use crate::{
    PUBLIC_UPLOADS, STORAGE,
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
}

#[derive(Debug)]
struct UploadState {
    batch_id: String,
    user_id: i32,
    total_size: u64,
    ranges: Vec<Range<u64>>,
    finalizing: bool,
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
    drop(state);

    Ok(Some(upload.clone()))
}

async fn persist_upload(upload: &Upload, state: &UploadState) -> Result<(), NurError> {
    let persisted = PersistedUpload {
        user_id: state.user_id,
        total_size: state.total_size,
        ranges: state.ranges.iter().map(|r| (r.start, r.end)).collect(),
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
        drop(state);
        return Ok(upload.clone());
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

        UploadState {
            batch_id: batch_id.to_string(),
            user_id,
            total_size,
            ranges,
            finalizing: false,
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

    let (width, height) = if mime_type.starts_with("image") {
        match image::open(temp_file) {
            Ok(img) => (Some(img.width() as i32), Some(img.height() as i32)),
            Err(_) => (None, None),
        }
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
    let path = Path::new(&media_path);

    let old_path = path
        .strip_prefix(PUBLIC_UPLOADS)
        .map(|p| Path::new(STORAGE.as_str()).join(p))
        .unwrap_or_else(|_| Path::new(STORAGE.as_str()).join(path))
        .join(&filename);

    let new_path = old_path.with_file_name(new_filename);

    // Rename the main file
    if old_path.exists() {
        fs::rename(&old_path, &new_path).await?;
        info!("Renamed file: {:?} -> {:?}", old_path, new_path);
    } else {
        return Err(NurError::Conflict(format!(
            "File not found: {:?}",
            old_path
        )));
    }

    // Rename variants if they exist
    let variant_dir = old_path
        .parent()
        .ok_or_else(|| NurError::Conflict("Invalid file path".into()))?;

    let old_stem = Path::new(&filename)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let new_stem = Path::new(new_filename)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();

    for variant in &mut media.variants {
        // Check if this is a variant of the original file
        if filename.starts_with(&*old_stem) && variant.filename != filename {
            let new_variant_name = variant.filename.replacen(&*old_stem, &new_stem, 1);
            let old_variant_path = variant_dir.join(&variant.filename);
            let new_variant_path = variant_dir.join(&new_variant_name);

            match fs::rename(&old_variant_path, &new_variant_path).await {
                Ok(_) => {
                    variant.filename = new_variant_name;
                    info!("Renamed variant: {old_variant_path:?} -> {new_variant_path:?}");
                }
                Err(e) => error!("Failed to rename variant {old_variant_path:?}: {e}"),
            }
        }
    }

    Ok(())
}

pub async fn delete_media_file(media: &MediaSerializer) -> Result<(), NurError> {
    let fname = media.filename.clone().unwrap_or_default();
    let rel = media.path.clone().unwrap_or_default();
    let rel_path = Path::new(&rel);

    // Resolve absolute path in storage
    let base_dir = match rel_path.strip_prefix(PUBLIC_UPLOADS) {
        Ok(p) => Path::new(STORAGE.as_str()).join(p),
        Err(_) => Path::new(STORAGE.as_str()).join(rel_path),
    };
    let target = base_dir.join(&fname);

    if !target.exists() {
        return Err(NurError::Conflict(format!("File not found: {:?}", target)));
    }

    // Delete variants first
    let stem = Path::new(&fname)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let parent = target
        .parent()
        .ok_or_else(|| NurError::Conflict("Invalid file path".into()))?;

    if let Ok(mut rd) = fs::read_dir(parent).await {
        while let Ok(Some(entry)) = rd.next_entry().await {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with(&stem) && name_str != fname {
                let variant_path = entry.path();
                if let Err(e) = fs::remove_file(&variant_path).await {
                    error!("Failed to remove variant {:?}: {}", variant_path, e);
                } else {
                    info!("Removed variant {:?}", variant_path);
                }
            }
        }
    }

    // Delete main file
    fs::remove_file(&target).await?;
    info!("Removed file {:?}", target);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_upload_complete, merge_ranges, uploading_path};
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
}
