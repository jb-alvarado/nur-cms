use std::{collections::HashMap, ops::Range, path::PathBuf, sync::LazyLock};

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

type UploadMap = HashMap<String, Vec<Range<u64>>>;
static UPLOADS: LazyLock<Mutex<UploadMap>> = LazyLock::new(|| Mutex::new(HashMap::new()));

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

pub async fn upload_chunk(
    details: AuthDetails<Role>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ServiceError> {
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

    if chunk_data.len() as u64 != end - start {
        return Err(ServiceError::BadRequest("Chunk length mismatch".into()));
    }

    let mut output_path = PathBuf::from(&*STORAGE);
    let year_month = Local::now().format("%Y/%m").to_string();
    output_path = output_path.join(&year_month);

    if !output_path.is_dir() {
        fs::create_dir_all(&output_path).await?;
    }

    let output_file = output_path.join(&file_name);
    let mut uploads = UPLOADS.lock().await;

    // Only conflict if file exists and no ongoing upload (.parts) is present
    if size > 0
        && fs::metadata(&output_file)
            .await
            .is_ok_and(|f| f.len() == size)
        && !uploads.contains_key(&file_name)
    {
        return Err(ServiceError::Conflict(format!(
            "File {file_name:?} already exists!"
        )));
    }

    if start == 0 {
        if uploads.contains_key(&file_name) {
            uploads.remove(&file_name);
            warn!("Remove existing file history for {file_name:?}");
        };

        info!("Start uploading: {output_file:?}");
    }
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&output_file)
        .await?;

    file.seek(SeekFrom::Start(start)).await?;
    file.write_all(&chunk_data).await?;
    file.flush().await?;

    let ranges = uploads.entry(file_name.clone()).or_default();
    ranges.push(start..end);
    merge_ranges(ranges);

    if fs::metadata(&output_file).await?.len() == size && is_upload_complete(ranges, size) {
        info!("Upload complete!");

        uploads.remove(&file_name);
    }

    Ok(StatusCode::OK)
}
