use axum::{
    Json,
    extract::{OriginalUri, Path, Query, State},
};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use serde_json::json;
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::db::{
    fields::{MediaFields, Table},
    handles,
    models::Role,
    queries::{QueryObj, RespondObj},
    serialize::*,
};
use crate::file::helper::{delete_media_file, rename_media_file};
use crate::sse::{SSELevel as Level, SSEMessage};
use crate::utils::errors::NurError;

pub async fn media_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<MediaFields>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<MediaSerializer>>, NurError> {
    params.path = original_uri.path().into();
    params.query = original_uri.query().unwrap_or("").into();

    if details.has_any_authority(&[&Role::Admin, &Role::Author, &Role::User]) {
        return match handles::select_media(&pool, &params).await {
            Ok(media) => Ok(Json(media)),
            Err(e) => {
                error!("{e}");
                Err(NurError::InternalServerError)
            }
        };
    }

    Err(NurError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}

pub async fn media_delete(
    State((pool, tx)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        let params: QueryObj<MediaFields> = QueryObj {
            fields: vec![
                MediaFields::Filename,
                MediaFields::Path,
                MediaFields::MediaVariants,
            ],
            search_id: Some(id),
            ..Default::default()
        };
        let media = handles::select_media(&pool, &params).await?;

        if let Some(m) = media.results.first()
            && let Err(e) = delete_media_file(m).await
        {
            let msg = SSEMessage::new(Level::Success, &format!("{e}"));
            let _ = tx.send(msg.to_string());
        }

        return match handles::delete_record(&pool, &Table::Media, id).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(NurError::InternalServerError)
            }
        };
    }

    Err(NurError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}

pub async fn media_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(content): Json<serde_json::Value>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        let params: QueryObj<MediaFields> = QueryObj {
            fields: vec![
                MediaFields::Filename,
                MediaFields::Path,
                MediaFields::MediaVariants,
            ],
            search_id: Some(id),
            ..Default::default()
        };
        let mut media = handles::select_media(&pool, &params).await?;

        if let Some(name) = content.get("filename").and_then(|v| v.as_str())
            && let Some(m) = media.results.first_mut()
            && m.filename.as_deref() != Some(name)
        {
            rename_media_file(m, name).await?;

            for variant in &m.variants {
                let data = json!({"filename": variant.filename});

                if let Err(e) =
                    handles::update_record(&pool, &Table::MediaVariants, variant.id, &data).await
                {
                    error!("{e}");
                }
            }
        }

        return match handles::update_record(&pool, &Table::Media, id, &content).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(NurError::InternalServerError)
            }
        };
    }

    Err(NurError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}
