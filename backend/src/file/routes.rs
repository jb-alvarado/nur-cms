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

#[derive(Clone)]
struct UploadMeta {
    batch_id: String,
    db_id: Option<i32>,
    mime_type: Option<String>,
}

impl UploadMeta {
    fn new(batch_id: String) -> Self {
        Self {
            batch_id,
            db_id: None,
            mime_type: None,
        }
    }
}

// Track byte ranges for each file being uploaded to support resumable uploads
type UploadValue = (Arc<Mutex<Vec<Range<u64>>>>, UploadMeta);
type UploadMap = HashMap<String, UploadValue>;
static UPLOADS: LazyLock<Mutex<UploadMap>> = LazyLock::new(|| Mutex::new(HashMap::new()));

// Merge overlapping or adjacent byte ranges to simplify tracking
fn merge_ranges(ranges: &mut Vec<Range<u64>>) {
    ranges.sort_by_key(|r| r.start);
    let mut merged: Vec<Range<u64>> = vec![];

    for r in ranges.drain(..) {
        if let Some(last) = merged.last_mut() {
            if last.end >= r.start {
                last.end = last.end.max(r.end);
            } else {
                merged.push(r);
            }
        } else {
            merged.push(r);
        }
    }
    *ranges = merged;
}

// Check if all chunks have been received by verifying contiguous ranges from 0 to total_size
fn is_upload_complete(ranges: &[Range<u64>], total_size: u64) -> bool {
    if ranges.is_empty() {
        return false;
    }

    let mut pos = 0;
    for r in ranges {
        if r.start != pos {
            return false; // gap
        }
        pos = r.end;
    }
    pos == total_size
}

fn is_batch_complete(upload_map: &UploadMap, batch_id: &String, batch_count: usize) -> bool {
    upload_map
        .values()
        .filter(|(_, meta)| meta.batch_id == *batch_id)
        .count()
        == batch_count
}

async fn file_ranges(
    start: u64,
    size: u64,
    file_name: &str,
    output_file: &Path,
    meta: UploadMeta,
) -> Result<UploadValue, ServiceError> {
    let upload_key = output_file.to_string_lossy().to_string();
    let mut uploads = UPLOADS.lock().await;

    // Prevent overwriting if file already exists and is not being tracked
    if size > 0
        && fs::metadata(&output_file)
            .await
            .is_ok_and(|f| f.len() == size)
        && !uploads.contains_key(&upload_key)
    {
        return Err(ServiceError::Conflict(format!(
            "File {file_name:?} already exists!"
        )));
    }

    // Only remove old tracking if it's a leftover from a previous interrupted upload
    if start == 0 {
        let mut remove_old = false;

        if let Some(upload_value) = uploads.get(&upload_key) {
            let ranges = upload_value.0.lock().await;
            if ranges.is_empty() {
                remove_old = true;
            }
        }

        if remove_old {
            // Old entry is not active → remove it
            uploads.remove(&upload_key);
            warn!("Removed old upload history for {file_name:?}");
        }

        info!("Start uploading: {output_file:?}");
    }

    // Get or create the current upload tracking
    Ok(uploads
        .entry(upload_key.clone())
        .or_insert_with(|| (Arc::new(Mutex::new(Vec::new())), meta))
        .clone())
}

async fn cleanup_uploads(batch_id: &str) {
    let mut uploads = UPLOADS.lock().await;
    uploads.retain(|_, (_, meta)| meta.batch_id != batch_id);
}

async fn add_media_record(
    pool: &PgPool,
    user_id: i32,
    output_file: &PathBuf,
) -> Result<(), ServiceError> {
    let upload_key = output_file.to_string_lossy().to_string();
    let mime = mime_guess::from_path(output_file).first();
    let mime_type = mime
        .map(|m| m.type_().to_string())
        .unwrap_or_else(|| mime_guess::mime::APPLICATION.to_string());

    let suffix = output_file
        .strip_prefix(STORAGE.as_str())
        .unwrap_or_else(|_| output_file);
    let path = Path::new(PUBLIC_UPLOADS)
        .join(suffix)
        .parent()
        .unwrap()
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

    if let Some((_, meta)) = uploads.get_mut(&upload_key) {
        meta.db_id = Some(media_id);
        meta.mime_type = Some(mime_type.clone());
    }

    Ok(())
}

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
        .filter_map(|(path_str, (_, meta))| {
            if meta.batch_id == batch_id {
                Some((PathBuf::from(path_str), meta.db_id?, meta.mime_type?))
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
            error!("{e}");
        };
    }

    for (output_file, media_id, mime_type) in &batch_files {
        if mime_type == "image" {
            match save_image(resolutions.clone(), &extensions, output_file, tx.clone()) {
                Ok(variances) => {
                    for (width, height, filename) in variances {
                        let pool = pool.clone();
                        let variance = MediaVariant {
                            id: 0,
                            media_id: *media_id,
                            width,
                            height,
                            filename,
                            total_count: None,
                        };

                        tokio::spawn(async move {
                            if let Err(e) = handles::insert_record::<MediaVariant, i64>(
                                &pool,
                                &Table::MediaVariants,
                                &variance,
                            )
                            .await
                            {
                                error!("{e}");
                            }
                        });
                    }
                }
                Err(e) => error!("{e}"),
            };
        }
    }

    Ok(())
}

// Handle chunked file uploads with support for resumable uploads
pub async fn upload_chunk(
    State((pool, tx)): State<(PgPool, Sender<String>)>,
    Extension(user): Extension<AuthUserMeta>,
    details: AuthDetails<Role>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ServiceError> {
    // Only admins and authors can upload files
    if !details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return Err(ServiceError::Forbidden(
            "You do not have permission to access this resource.".to_string(),
        ));
    }

    let mut file_name = None;
    let mut start: Option<u64> = None;
    let mut end: Option<u64> = None;
    let mut size = 0;
    let mut chunk_data: Option<Vec<u8>> = None;
    let mut batch_id = String::new();
    let mut batch_count = 0;

    // Extract multipart form fields: fileName, start, end, size, and chunk data
    while let Some(field) = multipart.next_field().await.ok().flatten() {
        match field.name().unwrap_or_default() {
            "fileName" => file_name = Some(sanitize(&field.text().await?)),
            "start" => start = Some(field.text().await?.parse::<u64>().unwrap_or(0)),
            "end" => end = Some(field.text().await?.parse::<u64>().unwrap_or(0)),
            "size" => size = field.text().await?.parse::<u64>().unwrap_or(0),
            "chunk" => chunk_data = Some(field.bytes().await?.to_vec()),
            "batch_id" => batch_id = field.text().await?.parse::<String>().unwrap_or("n0".into()),
            "batch_count" => batch_count = field.text().await?.parse::<usize>().unwrap_or(1),
            _ => {}
        }
    }

    let file_name = file_name.ok_or_else(|| ServiceError::BadRequest("Missing filename".into()))?;
    let start = start.ok_or_else(|| ServiceError::BadRequest("Missing start offset".into()))?;
    let end = end.ok_or_else(|| ServiceError::BadRequest("Missing end offset".into()))?;
    let chunk_data = chunk_data.ok_or_else(|| ServiceError::BadRequest("Missing chunk".into()))?;

    // Validate chunk size matches the declared range
    if chunk_data.len() as u64 != end - start {
        return Err(ServiceError::BadRequest("Chunk length mismatch".into()));
    }

    // Create storage path with year/month structure (e.g., 2025/11)
    let mut output_path = PathBuf::from(&*STORAGE);
    let year_month = Local::now().format("%Y/%m").to_string();
    output_path = output_path.join(&year_month);

    if !output_path.is_dir() {
        fs::create_dir_all(&output_path).await?;
    }

    let output_file = output_path.join(&file_name);
    let meta = UploadMeta::new(batch_id.clone());
    let upload_value_mutex = file_ranges(start, size, &file_name, &output_file, meta).await?;

    // Write chunk to the correct position in the file
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&output_file)
        .await?;

    file.seek(SeekFrom::Start(start)).await?;
    file.write_all(&chunk_data).await?;
    file.flush().await?;

    let is_complete = {
        let mut ranges = upload_value_mutex.0.lock().await;
        ranges.push(start..end);
        merge_ranges(&mut ranges);
        is_upload_complete(&ranges, size)
    };

    if is_complete {
        info!("Upload complete for file: {file_name}");

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
                SSEMessage::new(Level::Success, &format!("Uploaded done for: {file_name}"))
            };
            if let Err(e) = tx.send(msg.to_string()) {
                error!("{e}");
            };

            let config = CONFIG.read().await.clone();

            if config
                .image_extensions
                .as_ref()
                .is_some_and(|e| !e.is_empty())
            {
                task::spawn_blocking(move || {
                    if let Err(e) = process_variances(pool, config, uploads_clone, &batch_id, tx) {
                        error!("Error processing variances: {e}");
                    };

                    tokio::spawn(async move {
                        cleanup_uploads(&batch_id).await;
                    });
                });
            }
        }
    }

    Ok(StatusCode::OK)
}
