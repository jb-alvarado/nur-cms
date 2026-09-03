use axum::{
    Json,
    extract::{OriginalUri, Path, Query, State},
};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::db::{
    fields::{Table, VideoProfileFields},
    handles,
    models::{Role, VideoProfile},
    queries::{QueryObj, RespondObj},
};
use crate::file::video::validate_video_profile;
use crate::utils::errors::NurError;

pub async fn video_profile_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<VideoProfileFields>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<VideoProfile>>, NurError> {
    if !details.has_any_authority(&[&Role::Admin]) {
        return Err(NurError::Forbidden(
            "You do not have permission to access this resource.".into(),
        ));
    }

    params.path = original_uri.path().into();
    params.query = original_uri.query().unwrap_or("").into();

    match handles::select_record(&pool, &Table::VideoProfiles, params).await {
        Ok(profiles) => Ok(Json(profiles)),
        Err(e) => {
            error!("{e}");
            Err(NurError::InternalServerError)
        }
    }
}

pub async fn video_profile_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::delete_video_profile(&pool, id).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(e)
            }
        };
    }

    Err(NurError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}

pub async fn video_profile_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(profile): Json<VideoProfile>,
) -> Result<Json<i32>, NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        validate_video_profile(&profile).map_err(NurError::UnprocessableEntity)?;

        return handles::insert_video_profile(&pool, &profile)
            .await
            .map(Json);
    }

    Err(NurError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}

pub async fn video_profile_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(profile): Json<VideoProfile>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        validate_video_profile(&profile).map_err(NurError::UnprocessableEntity)?;

        return handles::update_video_profile(&pool, id, &profile).await;
    }

    Err(NurError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}
