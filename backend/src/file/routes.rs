use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

use axum::{
    Extension,
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::Local;
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use sanitize_filename::sanitize;
use sqlx::postgres::PgPool;
use tokio::{
    fs::{self, OpenOptions},
    io::{AsyncSeekExt, AsyncWriteExt, SeekFrom},
    sync::{Mutex, broadcast::Sender},
    task,
};
use tracing::{error, info, warn};

use crate::{
    AuthUserMeta, CONFIG, PUBLIC_UPLOADS, STORAGE,
    db::{
        fields::Table,
        handles,
        models::{Configuration, Media, MediaVariant, Role},
    },
    file::processing::save_image,
    sse::{SSELevel as Level, SSEMessage},
    utils::errors::ServiceError,
};

/// Metadata for a single file upload
#[derive(Clone)]
struct Meta {
    batch_id: String,
    db_id: Option<i32>,
    mime_type: Option<String>,
}

impl Meta {
    fn new(batch_id: String) -> Self {
        Self {
            batch_id,
            db_id: None,
            mime_type: None,
        }
    }
}

#[derive(Clone)]
struct Upload {
    ranges: Arc<Mutex<Vec<Range<u64>>>>,
    meta: Meta,
}

impl Upload {
    fn new(meta: Meta) -> Self {
        Self {
            ranges: Arc::new(Mutex::new(Vec::new())),
            meta,
        }
    }
}

/// Tracks byte ranges for resumable uploads
type UploadMap = HashMap<String, Upload>;

/// Global upload map, protected by a Mutex
static UPLOADS: LazyLock<Mutex<UploadMap>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Merge overlapping or adjacent ranges
fn merge_ranges(ranges: &mut Vec<Range<u64>>) {
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
fn is_upload_complete(ranges: &[Range<u64>], total_size: u64) -> bool {
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
fn is_batch_complete(upload_map: &UploadMap, batch_id: &str, batch_count: usize) -> bool {
    upload_map
        .values()
        .filter(|upload| upload.meta.batch_id == batch_id)
        .count()
        == batch_count
}

/// Get or create UploadValue for a file
async fn file_ranges(
    start: u64,
    total_size: u64,
    file_name: &str,
    output_file: &Path,
    meta: Meta,
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
        .or_insert_with(|| Upload::new(meta.clone()));

    let result = Upload {
        ranges: upload_entry.ranges.clone(),
        meta: upload_entry.meta.clone(),
    };

    drop(uploads);
    Ok(result)
}

/// Remove all uploads of a batch
async fn cleanup_uploads(batch_id: &str) {
    let mut uploads = UPLOADS.lock().await;
    uploads.retain(|_, upload| upload.meta.batch_id != batch_id);
}

/// Add media record to database and update UploadMeta
async fn add_media_record(
    pool: &PgPool,
    user_id: i32,
    output_file: &PathBuf,
) -> Result<(), ServiceError> {
    let upload_key = output_file.to_string_lossy().to_string();
    let mime_type = mime_guess::from_path(output_file)
        .first()
        .map(|m| m.type_().to_string())
        .unwrap_or_else(|| mime_guess::mime::APPLICATION.to_string());

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
        uploaded_by: Some(user_id),
        ..Default::default()
    };

    let media_id: i32 = handles::insert_record(pool, &Table::Media, &data).await?;

    let mut uploads = UPLOADS.lock().await;
    if let Some(upload) = uploads.get_mut(&upload_key) {
        upload.meta.db_id = Some(media_id);
        upload.meta.mime_type = Some(mime_type);
    }

    Ok(())
}

/// Process image variances in a batch
fn process_variances(
    pool: PgPool,
    config: Configuration,
    upload_map: UploadMap,
    batch_id: &str,
    tx: Sender<String>,
) -> Result<(), ServiceError> {
    let resolutions = config.image_resolutions.unwrap_or_default();
    let extensions = config.image_extensions.unwrap_or_default();

    let batch_files: Vec<(PathBuf, i32, String)> = upload_map
        .into_iter()
        .filter_map(|(path_str, upload)| {
            if upload.meta.batch_id == batch_id {
                Some((
                    PathBuf::from(path_str),
                    upload.meta.db_id?,
                    upload.meta.mime_type?,
                ))
            } else {
                None
            }
        })
        .collect();

    if batch_files
        .iter()
        .any(|(_, _, mime_type)| mime_type == "image")
    {
        let msg = SSEMessage::new(Level::Info, "Create image variances in background.");
        if let Err(e) = tx.send(msg.to_string()) {
            error!("SSE send failed: {e}");
        }
    }

    // Process each image sequentially to avoid excessive task spawning
    for (output_file, media_id, mime_type) in batch_files {
        if mime_type == "image" {
            match save_image(resolutions.clone(), &extensions, &output_file, tx.clone()) {
                Ok(variances) => {
                    for (width, height, filename) in variances {
                        let pool = pool.clone();
                        let variance = MediaVariant {
                            id: 0,
                            media_id,
                            width,
                            height,
                            filename,
                            total_count: None,
                        };

                        // Spawn a task for DB insert but limit concurrency if needed
                        tokio::spawn(async move {
                            if let Err(e) = handles::insert_record::<MediaVariant, i64>(
                                &pool,
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
                Err(e) => error!("Error saving image variances: {e}"),
            }
        }
    }

    Ok(())
}

/// Handle chunked/resumable file uploads
pub async fn upload_chunk(
    State((pool, tx)): State<(PgPool, Sender<String>)>,
    Extension(user): Extension<AuthUserMeta>,
    details: AuthDetails<Role>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ServiceError> {
    if !details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return Err(ServiceError::Forbidden(
            "You do not have permission to access this resource.".into(),
        ));
    }

    let mut file_name: Option<String> = None;
    let mut start: Option<u64> = None;
    let mut end: Option<u64> = None;
    let mut size: u64 = 0;
    let mut chunk_data: Option<Vec<u8>> = None;
    let mut batch_id = String::new();
    let mut batch_count = 1;

    // Extract multipart fields
    while let Some(field) = multipart.next_field().await.ok().flatten() {
        match field.name().unwrap_or_default() {
            "fileName" => file_name = Some(sanitize(&field.text().await?)),
            "start" => start = Some(field.text().await?.parse::<u64>().unwrap_or(0)),
            "end" => end = Some(field.text().await?.parse::<u64>().unwrap_or(0)),
            "size" => size = field.text().await?.parse::<u64>().unwrap_or(0),
            "chunk" => chunk_data = Some(field.bytes().await?.to_vec()),
            "batch_id" => batch_id = field.text().await?.clone(),
            "batch_count" => batch_count = field.text().await?.parse::<usize>().unwrap_or(1),
            _ => {}
        }
    }

    let file_name = file_name.ok_or_else(|| ServiceError::BadRequest("Missing filename".into()))?;
    let start = start.ok_or_else(|| ServiceError::BadRequest("Missing start offset".into()))?;
    let end = end.ok_or_else(|| ServiceError::BadRequest("Missing end offset".into()))?;
    let chunk_data = chunk_data.ok_or_else(|| ServiceError::BadRequest("Missing chunk".into()))?;

    // Validate chunk
    if end <= start || chunk_data.len() as u64 != end - start || end > size {
        return Err(ServiceError::BadRequest("Invalid chunk range".into()));
    }

    // Storage path: YEAR/MONTH
    let mut output_path = PathBuf::from(&*STORAGE);
    output_path = output_path.join(Local::now().format("%Y/%m").to_string());
    if !output_path.is_dir() {
        fs::create_dir_all(&output_path).await?;
    }

    let output_file = output_path.join(&file_name);
    let meta = Meta::new(batch_id.clone());
    let upload_value = file_ranges(start, size, &file_name, &output_file, meta).await?;

    // Write chunk
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&output_file)
        .await?;
    file.seek(SeekFrom::Start(start)).await?;
    file.write_all(&chunk_data).await?;
    file.flush().await?;

    // Update ranges and check completion
    let ranges_arc = upload_value.ranges.clone();
    let mut ranges = ranges_arc.lock().await;
    ranges.push(start..end);

    merge_ranges(&mut ranges);

    let is_complete = is_upload_complete(&ranges, size);

    if is_complete {
        info!("Upload complete: {file_name}");
        add_media_record(&pool, user.id, &output_file).await?;

        let uploads = UPLOADS.lock().await;
        if is_batch_complete(&uploads, &batch_id, batch_count) {
            let uploads_clone = uploads.clone();

            let msg = if batch_count > 1 {
                SSEMessage::new(
                    Level::Success,
                    &format!("Batch upload complete: {batch_count} files uploaded."),
                )
            } else {
                SSEMessage::new(Level::Success, &format!("Upload done: {file_name}"))
            };

            if let Err(e) = tx.send(msg.to_string()) {
                error!("SSE send failed: {e}");
            }

            let config = CONFIG.read().await.clone();

            if config
                .image_extensions
                .as_ref()
                .map(|v| !v.is_empty())
                .unwrap_or(false)
            {
                // Spawn blocking for CPU-intensive image processing
                task::spawn_blocking(move || {
                    if let Err(e) = process_variances(pool, config, uploads_clone, &batch_id, tx) {
                        error!("Error processing variances: {e}");
                    }
                    tokio::spawn(async move {
                        cleanup_uploads(&batch_id).await;
                    });
                });
            }
        }
    }

    Ok(StatusCode::OK)
}
