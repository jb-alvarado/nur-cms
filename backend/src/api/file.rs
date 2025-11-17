use std::{
    collections::HashMap,
    ops::Range,
    path::PathBuf,
    sync::{Arc, LazyLock},
};

use axum::{extract::Multipart, http::StatusCode, response::IntoResponse};
use chrono::Local;
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use sanitize_filename::sanitize;
use tokio::{
    fs::{self, OpenOptions},
    io::{AsyncSeekExt, AsyncWriteExt, SeekFrom},
    sync::Mutex,
};
use tracing::{info, warn};

use crate::{STORAGE, db::models::Role, utils::errors::ServiceError};

// Track byte ranges for each file being uploaded to support resumable uploads
type FileRanges = Arc<Mutex<Vec<Range<u64>>>>;
type UploadMap = HashMap<String, FileRanges>;
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

// Handle chunked file uploads with support for resumable uploads
pub async fn upload_chunk(
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

    // Extract multipart form fields: fileName, start, end, size, and chunk data
    while let Some(field) = multipart.next_field().await.ok().flatten() {
        match field.name().unwrap_or_default() {
            "fileName" => file_name = Some(sanitize(&field.text().await?)),
            "start" => start = Some(field.text().await?.parse::<u64>().unwrap_or(0)),
            "end" => end = Some(field.text().await?.parse::<u64>().unwrap_or(0)),
            "size" => size = field.text().await?.parse::<u64>().unwrap_or(0),
            "chunk" => chunk_data = Some(field.bytes().await?.to_vec()),
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
    let output_str = output_file.to_string_lossy().to_string();

    let file_ranges_mutex = {
        let mut uploads = UPLOADS.lock().await;

        // Prevent overwriting if file already exists and is not being tracked
        if size > 0
            && fs::metadata(&output_file)
                .await
                .is_ok_and(|f| f.len() == size)
            && !uploads.contains_key(&output_str)
        {
            return Err(ServiceError::Conflict(format!(
                "File {file_name:?} already exists!"
            )));
        }

        // Reset upload tracking when starting a new upload (start == 0)
        if start == 0 {
            if uploads.contains_key(&output_str) {
                uploads.remove(&output_str);
                warn!("Remove existing file history for {file_name:?}");
            };

            info!("Start uploading: {output_file:?}");
        }

        uploads
            .entry(output_str.clone())
            .or_insert_with(|| Arc::new(Mutex::new(Vec::new())))
            .clone()
    };

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

    // Update range tracking and check if upload is complete
    {
        let mut ranges = file_ranges_mutex.lock().await;
        ranges.push(start..end);
        merge_ranges(&mut ranges);

        if is_upload_complete(&ranges, size) {
            info!("Upload complete!");
            let mut uploads = UPLOADS.lock().await;
            uploads.remove(&output_str);

            // Clean up incomplete or invalid uploads from tracking map
            uploads.retain(|path_str, ranges_arc| {
                let file_path = PathBuf::from(path_str);

                let Ok(meta) = std::fs::metadata(&file_path) else {
                    return false;
                };

                if meta.len() == 0 {
                    return false;
                }

                if let Ok(ranges) = ranges_arc.try_lock() {
                    return !is_upload_complete(&ranges, meta.len());
                }

                true
            });
        }
    }

    Ok(StatusCode::OK)
}
