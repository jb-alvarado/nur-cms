use axum::{
    Json,
    extract::{Path, State},
};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use sqlx::{Connection, PgPool};
use tokio::{fs, sync::broadcast::Sender};

use crate::{
    db::{handles, models::Role},
    file::{
        helper::{contained_storage_target, hls_directory_name},
        subtitles,
    },
    utils::errors::NurError,
};

fn authorize(details: &AuthDetails<Role>) -> Result<(), NurError> {
    if !details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return Err(NurError::Forbidden(
            "You do not have permission to manage subtitles.".into(),
        ));
    }
    Ok(())
}

pub async fn subtitle_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<handles::MediaSubtitle>>, NurError> {
    authorize(&details)?;
    let mut tx = handles::lock_media_files(&pool, id).await?;
    subtitles::recover_publication_on(&mut tx, id).await?;
    if handles::subtitle_target(&mut tx, id).await?.is_none() {
        return Err(NurError::NotFound);
    }
    let tracks = handles::select_subtitles_on(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(tracks))
}

pub async fn subtitle_upload(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(mut input): Json<handles::SubtitleUpload>,
) -> Result<Json<Vec<handles::MediaSubtitle>>, NurError> {
    authorize(&details)?;
    subtitles::validate_upload(&mut input)?;
    change(&pool, id, Some(input), None).await.map(Json)
}

pub async fn subtitle_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path((id, track)): Path<(i32, String)>,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<handles::MediaSubtitle>>, NurError> {
    authorize(&details)?;
    change(&pool, id, None, Some(&track)).await.map(Json)
}

async fn change(
    pool: &PgPool,
    id: i32,
    upload: Option<handles::SubtitleUpload>,
    remove: Option<&str>,
) -> Result<Vec<handles::MediaSubtitle>, NurError> {
    let mut connection = handles::lock_media_publication(pool, id).await?;
    let mut tx = connection.begin().await?;
    subtitles::recover_publication_on(&mut tx, id).await?;
    let target = handles::subtitle_target(&mut tx, id)
        .await?
        .ok_or_else(|| NurError::BadRequest("The media item has no HLS output.".into()))?;
    if target.processing_status != "completed"
        || handles::media_has_active_video_job(&mut tx, id).await?
    {
        return Err(NurError::Conflict(
            "Wait for video processing to finish before changing subtitles.".into(),
        ));
    }
    let directory_name = hls_directory_name(&target.filename).ok_or(NurError::InvalidInput)?;
    let directory = contained_storage_target(&target.path, directory_name).await?;
    let master = fs::read_to_string(directory.join("master.m3u8")).await?;
    let (duration, mpegts) = subtitles::timeline(&directory, &master).await?;
    if let Some(upload) = upload {
        handles::save_subtitle(&mut tx, id, &upload).await?;
    }
    if let Some(track) = remove
        && !handles::delete_subtitle(&mut tx, id, track).await?
    {
        return Err(NurError::NotFound);
    }
    let tracks = handles::select_subtitles_on(&mut tx, id).await?;
    if tracks.len() > 32 {
        return Err(NurError::BadRequest(
            "A video may have at most 32 subtitle tracks.".into(),
        ));
    }
    let updated = subtitles::render_master(&master, &tracks)?;
    let limit = *crate::VIDEO_PROCESSING_MAX_OUTPUT_SIZE;
    let mut thumbnail_bytes = 0_u64;
    if limit.is_some() {
        for filename in handles::subtitle_thumbnail_filenames(&mut tx, id).await? {
            let path = contained_storage_target(&target.path, &filename).await?;
            match fs::metadata(path).await {
                Ok(metadata) => {
                    thumbnail_bytes = thumbnail_bytes
                        .checked_add(metadata.len())
                        .ok_or(NurError::InvalidInput)?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    subtitles::check_publication_size(
        &directory,
        &tracks,
        (duration, mpegts),
        &updated,
        thumbnail_bytes,
        limit,
    )
    .await?;
    subtitles::publish_tracks(&directory, &tracks, duration, mpegts).await?;
    let backup = subtitles::backup_master(&directory, &master).await?;
    handles::save_subtitle_publication(&mut tx, id, &master, &updated, &backup).await?;
    // The live master is unchanged until both the new tracks and durable intent
    // have committed. If commit has an uncertain outcome, leave recovery to a
    // subsequent lock holder; never restore stale filesystem state.
    tx.commit().await?;
    let mut tx = connection.begin().await?;
    subtitles::recover_publication_on(&mut tx, id).await?;
    tx.commit().await?;
    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use axum::extract::FromRequestParts;
    use protect_axum::authorities::AttachAuthorities;

    use super::*;

    async fn details(role: Role) -> AuthDetails<Role> {
        let mut request = axum::http::Request::new(());
        request.attach([role]);
        let (mut parts, _) = request.into_parts();
        AuthDetails::from_request_parts(&mut parts, &())
            .await
            .expect("authorities")
    }

    #[tokio::test]
    async fn unauthorized_roles_cannot_list_upload_or_delete_subtitles() {
        let pool = PgPool::connect_lazy("postgres://localhost/unused").expect("lazy pool");
        let (sender, _) = tokio::sync::broadcast::channel(1);
        for role in [Role::User, Role::Guest, Role::Custom("subscriber".into())] {
            assert!(matches!(
                subtitle_select(
                    State((pool.clone(), sender.clone())),
                    Path(1),
                    details(role.clone()).await
                )
                .await,
                Err(NurError::Forbidden(_))
            ));
            let input = handles::SubtitleUpload {
                language: "de".into(),
                label: "Deutsch".into(),
                is_default: false,
                webvtt: String::new(),
            };
            assert!(matches!(
                subtitle_upload(
                    State((pool.clone(), sender.clone())),
                    Path(1),
                    details(role.clone()).await,
                    Json(input)
                )
                .await,
                Err(NurError::Forbidden(_))
            ));
            assert!(matches!(
                subtitle_delete(
                    State((pool.clone(), sender.clone())),
                    Path((1, "track".into())),
                    details(role).await
                )
                .await,
                Err(NurError::Forbidden(_))
            ));
        }
        assert!(authorize(&details(Role::Admin).await).is_ok());
        assert!(authorize(&details(Role::Author).await).is_ok());
    }
}
