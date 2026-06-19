use std::path::{Path, PathBuf};

use axum::{
    Extension, Json,
    extract::{Multipart, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::Local;
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use sanitize_filename::sanitize;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use tokio::{fs, sync::broadcast::Sender};
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
            "File type '{}' is not allowed.",
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

#[derive(Deserialize)]
pub struct UploadStatusQuery {
    file_name: String,
    size: u64,
    batch_id: String,
}

#[derive(Serialize)]
pub struct UploadStatus {
    received_ranges: Vec<(u64, u64)>,
    complete: bool,
}

fn upload_output_file(file_name: &str) -> PathBuf {
    PathBuf::from(&*STORAGE)
        .join(Local::now().format("%Y/%m").to_string())
        .join(file_name)
}

fn public_upload_path(output_file: &Path) -> String {
    output_file
        .strip_prefix(STORAGE.as_str())
        .unwrap_or(output_file)
        .parent()
        .map(|path| Path::new(PUBLIC_UPLOADS).join(path))
        .unwrap_or_else(|| Path::new(PUBLIC_UPLOADS).to_path_buf())
        .to_string_lossy()
        .to_string()
}

async fn ensure_upload_directory(output_file: &Path) -> Result<(), NurError> {
    let output_path = output_file
        .parent()
        .ok_or_else(|| NurError::BadRequest("Invalid upload path".into()))?;

    match fs::metadata(output_path).await {
        Ok(meta) if meta.is_dir() => Ok(()),
        Ok(_) => Err(NurError::BadRequest(format!(
            "Upload path exists but is not a directory: {}",
            output_path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(output_path).await.map_err(|error| {
                error!(
                    "Failed to create upload directory {}: {}",
                    output_path.display(),
                    error
                );
                NurError::BadRequest(format!("Failed to create upload directory: {error}"))
            })?;
            info!("Created directory: {}", output_path.display());
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

pub async fn upload_status(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Extension(user): Extension<AuthUserMeta>,
    details: AuthDetails<Role>,
    Query(query): Query<UploadStatusQuery>,
) -> Result<Json<UploadStatus>, NurError> {
    if !details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return Err(NurError::Forbidden(
            "You do not have permission to access this resource.".into(),
        ));
    }

    let file_name = sanitize(&query.file_name);
    validate_mime_type(&file_name)?;
    if query.batch_id.is_empty() || query.size > *MAX_UPLOAD_SIZE {
        return Err(NurError::BadRequest("Invalid upload metadata".into()));
    }

    let output_file = upload_output_file(&file_name);
    ensure_upload_directory(&output_file).await?;
    let file_path = public_upload_path(&output_file);

    if let Some(existing_upload_id) = sqlx::query_scalar::<_, Option<String>>(
        "SELECT upload_id FROM media WHERE filename = $1 AND path = $2",
    )
    .bind(&file_name)
    .bind(&file_path)
    .fetch_optional(&pool)
    .await?
    {
        if existing_upload_id.as_deref() == Some(query.batch_id.as_str())
            && fs::try_exists(&output_file).await?
        {
            return Ok(Json(UploadStatus {
                received_ranges: Vec::new(),
                complete: true,
            }));
        }

        return Err(NurError::Conflict(format!(
            "File '{file_name}' already exists in database."
        )));
    }

    let upload = get_or_create_upload(query.size, &output_file, &query.batch_id, user.id).await?;

    Ok(Json(UploadStatus {
        received_ranges: received_ranges(&upload).await,
        complete: false,
    }))
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

    // Extract multipart fields
    while let Some(field) = multipart.next_field().await.ok().flatten() {
        match field.name().unwrap_or_default() {
            "fileName" => file_name = Some(sanitize(&field.text().await?)),
            "start" => start = Some(field.text().await?.parse::<u64>().unwrap_or(0)),
            "end" => end = Some(field.text().await?.parse::<u64>().unwrap_or(0)),
            "size" => size = field.text().await?.parse::<u64>().unwrap_or(0),
            "chunk" => chunk_data = Some(field.bytes().await?.to_vec()),
            "batch_id" => batch_id = field.text().await?,
            _ => {}
        }
    }

    let original_filename =
        file_name.ok_or_else(|| NurError::BadRequest("Missing filename".into()))?;
    let start = start.ok_or_else(|| NurError::BadRequest("Missing start offset".into()))?;
    let end = end.ok_or_else(|| NurError::BadRequest("Missing end offset".into()))?;
    let chunk_data = chunk_data.ok_or_else(|| NurError::BadRequest("Missing chunk".into()))?;
    if batch_id.is_empty() {
        return Err(NurError::BadRequest("Missing batch id".into()));
    }

    // Validate MIME type
    validate_mime_type(&original_filename)?;

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
            "Chunk size {} exceeds maximum allowed chunk size of {}",
            format_bytes(chunk_data.len() as u64),
            format_bytes(*MAX_CHUNK_SIZE)
        )));
    }

    // Validate chunk
    if end <= start || chunk_data.len() as u64 != end - start || end > size {
        return Err(NurError::BadRequest("Invalid chunk range".into()));
    }

    // Use sanitized original filename (DB check prevents overwrites)
    let file_name = original_filename;

    // Storage path: YEAR/MONTH
    let output_file = upload_output_file(&file_name);
    ensure_upload_directory(&output_file).await?;

    let file_path = public_upload_path(&output_file);

    let upload =
        if let Some(upload) = get_active_upload(&output_file, &batch_id, user.id, size).await? {
            upload
        } else {
            if let Some(existing_upload_id) = sqlx::query_scalar::<_, Option<String>>(
                "SELECT upload_id FROM media WHERE filename = $1 AND path = $2",
            )
            .bind(&file_name)
            .bind(&file_path)
            .fetch_optional(&pool)
            .await?
            {
                if existing_upload_id.as_deref() == Some(batch_id.as_str())
                    && fs::try_exists(&output_file).await?
                {
                    return Ok(StatusCode::OK);
                }

                return Err(NurError::Conflict(format!(
                    "File '{file_name}' already exists in database."
                )));
            }

            get_or_create_upload(size, &output_file, &batch_id, user.id).await?
        };
    let should_finalize = write_upload_chunk(&upload, start, end, &chunk_data).await?;

    if should_finalize {
        info!("Upload complete: {file_name}");
        let result = async {
            let (media_id, stored_mime_type, processable_image) =
                add_media_record(&pool, user.id, &batch_id, &upload.temp_file, &output_file)
                    .await?;

            if let Err(error) = fs::rename(&upload.temp_file, &output_file).await {
                delete_media_record(&pool, media_id).await;
                return Err(error.into());
            }

            let config = CONFIG.read().await.clone();
            if let Err(error) = process_variants(
                &pool,
                &config,
                &output_file,
                media_id,
                &stored_mime_type,
                processable_image,
                &tx,
            )
            .await
            {
                delete_media_record(&pool, media_id).await;
                if let Err(rename_error) = fs::rename(&output_file, &upload.temp_file).await {
                    error!("Failed to restore incomplete upload: {rename_error}");
                }
                return Err(error);
            }

            Ok::<_, NurError>(())
        }
        .await;

        match result {
            Ok(()) => {
                cleanup_upload(&output_file, &upload).await;
                let msg = SSEMessage::new(Level::Success, &format!("Upload done: {file_name}"));
                if let Err(error) = tx.send(msg.to_string()) {
                    error!("SSE send failed: {error}");
                }
            }
            Err(error) => {
                reset_finalizing(&upload).await;
                return Err(error);
            }
        }
    }

    Ok(StatusCode::OK)
}
