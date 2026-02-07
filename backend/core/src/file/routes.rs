use std::path::{Path, PathBuf};
use std::sync::Arc;

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
use tracing::{error, info};

use crate::{
    AuthUserMeta, CONFIG, MAX_CHUNK_SIZE, MAX_UPLOAD_SIZE, PUBLIC_UPLOADS, STORAGE,
    db::models::Role,
    file::helper::*,
    sse::{SSELevel as Level, SSEMessage},
    utils::errors::NurError,
};

// Allowed MIME types for uploads
const ALLOWED_MIME_TYPES: &[&str] = &[
    "application/epub+zip",
    "application/gzip",
    "application/json",
    "application/msword",
    "application/pdf",
    "application/rtf",
    "application/vnd.apple.keynote",
    "application/vnd.apple.numbers",
    "application/vnd.apple.pages",
    "application/vnd.ms-excel",
    "application/vnd.ms-outlook",
    "application/vnd.ms-powerpoint",
    "application/vnd.oasis.opendocument.presentation",
    "application/vnd.oasis.opendocument.spreadsheet",
    "application/vnd.oasis.opendocument.text",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/x-7z-compressed",
    "application/x-bzip2",
    "application/x-rar-compressed",
    "application/x-tar",
    "application/x-xz",
    "application/x-zip-compressed",
    "application/xml",
    "application/zip",
    "audio/aac",
    "audio/flac",
    "audio/mp4",
    "audio/mpeg",
    "audio/ogg",
    "audio/wav",
    "audio/webm",
    "image/avif",
    "image/gif",
    "image/heic",
    "image/heif",
    "image/jpeg",
    "image/jpg",
    "image/png",
    "image/svg+xml",
    "image/webp",
    "text/csv",
    "text/plain",
    "text/xml",
    "video/mp4",
    "video/ogg",
    "video/quicktime",
    "video/webm",
];

fn validate_mime_type(filename: &str) -> Result<String, NurError> {
    let mime_type = mime_guess::from_path(filename)
        .first_or_octet_stream()
        .to_string();

    if ALLOWED_MIME_TYPES.contains(&mime_type.as_str()) {
        Ok(mime_type)
    } else {
        Err(NurError::BadRequest(format!(
            "File type '{}' is not allowed. Only images, videos, audio, and PDF files are permitted.",
            mime_type
        )))
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * KB;
    const GB: f64 = 1024.0 * MB;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// Handle chunked/resumable file uploads
pub async fn upload_chunk(
    State((pool, tx)): State<(PgPool, Sender<String>)>,
    Extension(user): Extension<AuthUserMeta>,
    details: AuthDetails<Role>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, NurError> {
    if !details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return Err(NurError::Forbidden(
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
            "batch_id" => batch_id = field.text().await?,
            "batch_count" => batch_count = field.text().await?.parse::<usize>().unwrap_or(1),
            _ => {}
        }
    }

    let original_filename =
        file_name.ok_or_else(|| NurError::BadRequest("Missing filename".into()))?;
    let start = start.ok_or_else(|| NurError::BadRequest("Missing start offset".into()))?;
    let end = end.ok_or_else(|| NurError::BadRequest("Missing end offset".into()))?;
    let chunk_data = chunk_data.ok_or_else(|| NurError::BadRequest("Missing chunk".into()))?;

    // Validate MIME type
    let mime_type = validate_mime_type(&original_filename)?;

    // Validate file size limits
    if size > *MAX_UPLOAD_SIZE {
        return Err(NurError::BadRequest(format!(
            "File size {} exceeds maximum allowed size of {}",
            format_bytes(size),
            format_bytes(*MAX_UPLOAD_SIZE)
        )));
    }

    // Validate chunk size
    if chunk_data.len() as u64 > *MAX_CHUNK_SIZE {
        return Err(NurError::BadRequest(format!(
            "Chunk size {} bytes exceeds maximum allowed chunk size of {} bytes",
            chunk_data.len(),
            *MAX_CHUNK_SIZE
        )));
    }

    // Validate chunk
    if end <= start || chunk_data.len() as u64 != end - start || end > size {
        return Err(NurError::BadRequest("Invalid chunk range".into()));
    }

    // Use sanitized original filename (DB check prevents overwrites)
    let file_name = original_filename;

    // Storage path: YEAR/MONTH
    let mut output_path = PathBuf::from(&*STORAGE);
    output_path = output_path.join(Local::now().format("%Y/%m").to_string());
    if !output_path.is_dir() {
        fs::create_dir_all(&output_path).await?;
    }

    let output_file = output_path.join(&file_name);

    // On first chunk: check if file already exists in DB to prevent overwriting
    if start == 0 {
        let file_path = output_file
            .strip_prefix(STORAGE.as_str())
            .unwrap_or(&output_file)
            .parent()
            .map(|p| Path::new(PUBLIC_UPLOADS).join(p))
            .unwrap_or_else(|| Path::new(PUBLIC_UPLOADS).to_path_buf())
            .to_string_lossy()
            .to_string();

        if file_exists_in_db(&pool, &file_name, &file_path).await? {
            return Err(NurError::Conflict(format!(
                "File '{}' already exists in database. Cannot overwrite.",
                file_name
            )));
        }
    }
    let meta = Arc::new(Mutex::new(Meta {
        db_id: None,
        mime_type: Some(mime_type),
    }));
    let upload_value = file_ranges(start, size, &file_name, &output_file, &batch_id, meta).await?;

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

        // Check batch completion with proper locking
        let should_process_batch = {
            let uploads = UPLOADS.lock().await;
            is_batch_complete(&uploads, &batch_id, batch_count)
        };

        if should_process_batch {
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

            info!("Upload complete!");

            let config = CONFIG.read().await.clone();

            if config
                .image_extensions
                .as_ref()
                .is_some_and(|v| !v.is_empty())
            {
                let uploads_clone = {
                    let uploads = UPLOADS.lock().await;
                    uploads.clone()
                };

                let id_owned = batch_id.clone();
                let pool_clone = pool.clone();
                let txc = tx.clone();
                let handle = tokio::runtime::Handle::current();

                // Spawn blocking task for CPU-intensive image processing
                task::spawn_blocking(move || {
                    if let Err(e) =
                        process_variants(pool_clone, config, uploads_clone, id_owned.clone(), txc)
                    {
                        error!("Error processing variants: {e}");
                    }

                    // Cleanup in separate async task
                    handle.spawn(async move {
                        cleanup_uploads(&id_owned).await;

                        info!("Background job done!");
                    });
                });
            }
        }
    }

    Ok(StatusCode::OK)
}
