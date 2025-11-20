use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

use sqlx::postgres::PgPool;
use tokio::{
    fs,
    sync::{Mutex, broadcast::Sender},
};
use tracing::{error, info, warn};

use crate::{
    PUBLIC_UPLOADS, STORAGE,
    db::{
        fields::Table,
        handles,
        models::{Configuration, Media, MediaVariant},
    },
    file::processing::save_image,
    sse::{SSELevel as Level, SSEMessage},
    utils::errors::ServiceError,
};

/// Metadata for a single file upload
#[derive(Clone, Default)]
pub struct Meta {
    pub db_id: Option<i32>,
    pub mime_type: Option<String>,
}

#[derive(Clone)]
pub struct Upload {
    pub batch_id: String,
    pub ranges: Arc<Mutex<Vec<Range<u64>>>>,
    pub meta: Arc<Mutex<Meta>>,
}

impl Upload {
    pub fn new(batch_id: String, meta: Arc<Mutex<Meta>>) -> Self {
        Self {
            batch_id,
            ranges: Arc::new(Mutex::new(Vec::new())),
            meta,
        }
    }
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

/// Check if a batch of uploads is complete
pub fn is_batch_complete(upload_map: &UploadMap, batch_id: &str, batch_count: usize) -> bool {
    upload_map
        .values()
        .filter(|upload| upload.batch_id == batch_id)
        .count()
        == batch_count
}

/// Get or create UploadValue for a file
pub async fn file_ranges(
    start: u64,
    total_size: u64,
    file_name: &str,
    output_file: &Path,
    batch_id: &str,
    meta: Arc<Mutex<Meta>>,
) -> Result<Upload, ServiceError> {
    let upload_key = output_file.to_string_lossy().to_string();
    let mut uploads = UPLOADS.lock().await;

    // Prevent overwriting if file already exists and is not being tracked
    if total_size > 0
        && fs::metadata(&output_file)
            .await
            .is_ok_and(|f| f.len() == total_size)
        && !uploads.contains_key(&upload_key)
    {
        return Err(ServiceError::Conflict(format!(
            "File {file_name:?} already exists!"
        )));
    }

    // Remove old tracking if start == 0 and file has no active ranges
    if start == 0 {
        if let Some(upload) = uploads.get(&upload_key) {
            let is_empty = {
                let guard = upload.ranges.lock().await;
                guard.is_empty()
            };

            if is_empty {
                uploads.remove(&upload_key);
                warn!("Removed old upload history for {file_name:?}");
            }
        }

        info!("Start uploading: {output_file:?}");
    }

    let upload_entry = uploads
        .entry(upload_key.clone())
        .or_insert_with(|| Upload::new(batch_id.to_string(), meta.clone()));

    let result = Upload {
        batch_id: upload_entry.batch_id.clone(),
        ranges: upload_entry.ranges.clone(),
        meta: upload_entry.meta.clone(),
    };

    drop(uploads);
    Ok(result)
}

/// Remove all uploads of a batch
pub async fn cleanup_uploads(batch_id: &str) {
    let mut uploads = UPLOADS.lock().await;
    uploads.retain(|_, upload| upload.batch_id != batch_id);
}

/// Add media record to database and update UploadMeta
pub async fn add_media_record(
    pool: &PgPool,
    user_id: i32,
    output_file: &PathBuf,
) -> Result<(), ServiceError> {
    let upload_key = output_file.to_string_lossy().to_string();
    let mime_type = mime_guess::from_path(output_file)
        .first_or_octet_stream()
        .to_string();

    let (width, height) = if mime_type.starts_with("image") {
        let img = image::open(output_file)?;
        let width = img.width();
        let height = img.height();
        (Some(width as i32), Some(height as i32))
    } else {
        (None, None)
    };

    let size = output_file.metadata().map(|m| m.len() as i64).ok();

    let path = output_file
        .strip_prefix(STORAGE.as_str())
        .unwrap_or(output_file)
        .parent()
        .map(|p| Path::new(PUBLIC_UPLOADS).join(p))
        .ok_or_else(|| ServiceError::Conflict("Invalid file path".into()))?
        .to_string_lossy()
        .to_string();

    let data = Media {
        alt: Some(
            output_file
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        ),
        filename: output_file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        path,
        r#type: Some(mime_type.clone()),
        width,
        height,
        size,
        uploaded_by: Some(user_id),
        ..Default::default()
    };

    let media_id: i32 = handles::insert_record(pool, &Table::Media, &data).await?;

    let mut uploads = UPLOADS.lock().await;
    if let Some(upload) = uploads.get_mut(&upload_key) {
        let mut meta = upload.meta.lock().await;
        meta.db_id = Some(media_id);
        meta.mime_type = Some(mime_type);
    }

    Ok(())
}

/// Process image variants in a batch
pub fn process_variants(
    pool: PgPool,
    config: Configuration,
    upload_map: UploadMap,
    batch_id: String,
    tx: Sender<String>,
) -> Result<(), ServiceError> {
    let resolutions = config.image_resolutions.unwrap_or_default();
    let extensions = config.image_extensions.unwrap_or_default();

    let mut batch_files = Vec::new();
    for (path_str, upload) in upload_map {
        if upload.batch_id == batch_id {
            let meta = upload.meta.blocking_lock();
            if let (Some(db_id), Some(mime_type)) = (meta.db_id, meta.mime_type.clone()) {
                batch_files.push((PathBuf::from(path_str), db_id, mime_type));
            }
        }
    }

    if batch_files
        .iter()
        .any(|(_, _, mime_type)| mime_type.contains("image"))
    {
        let msg = SSEMessage::new(Level::Info, "Create image variants in background.");
        if let Err(e) = tx.send(msg.to_string()) {
            error!("SSE send failed: {e}");
        }
    }

    // Process each image sequentially to avoid excessive task spawning
    for (output_file, media_id, mime_type) in batch_files {
        if mime_type.contains("image") {
            match save_image(resolutions.clone(), &extensions, &output_file, tx.clone()) {
                Ok(variants) => {
                    for (width, height, filename) in variants {
                        let pool_clone = pool.clone();
                        let variance = MediaVariant {
                            id: 0,
                            media_id,
                            width,
                            height,
                            filename,
                            total_count: None,
                        };

                        // Spawn async task for DB insert
                        tokio::spawn(async move {
                            if let Err(e) = handles::insert_record::<MediaVariant, i64>(
                                &pool_clone,
                                &Table::MediaVariants,
                                &variance,
                            )
                            .await
                            {
                                error!("Error inserting MediaVariant: {e}");
                            }
                        });
                    }
                }
                Err(e) => error!("Error saving image variants: {e}"),
            }
        }
    }

    Ok(())
}
