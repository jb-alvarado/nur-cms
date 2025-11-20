use std::{path::PathBuf, sync::Arc};

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
    AuthUserMeta, CONFIG, STORAGE,
    db::models::Role,
    file::helper::*,
    sse::{SSELevel as Level, SSEMessage},
    utils::errors::ServiceError,
};

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
            "batch_id" => batch_id = field.text().await?,
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
    let meta = Arc::new(Mutex::new(Meta::default()));
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
