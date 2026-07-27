use std::path::Path as FsPath;

use axum::{
    Json,
    extract::{OriginalUri, Path, Query, State},
};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use serde_json::Value;
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
            let msg = SSEMessage::new(Level::Error, &format!("{e}"));
            let _ = tx.send(msg.to_string());
            return Err(e);
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
    Json(mut content): Json<Value>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        let object = content.as_object_mut().ok_or(NurError::InvalidInput)?;
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "alt" | "filename"))
        {
            return Err(NurError::InvalidInput);
        }
        if let Some(value) = object.get("alt")
            && !value.is_null()
            && value.as_str().is_none_or(|alt| alt.chars().count() > 2_000)
        {
            return Err(NurError::BadRequest("Invalid alternative text.".into()));
        }
        if let Some(value) = object.get("filename") {
            let filename = value
                .as_str()
                .ok_or_else(|| NurError::BadRequest("Invalid filename.".into()))?;
            let sanitized = sanitize_filename::sanitize(filename);
            if sanitized != filename
                || filename.is_empty()
                || FsPath::new(filename)
                    .file_name()
                    .and_then(|name| name.to_str())
                    != Some(filename)
            {
                return Err(NurError::BadRequest("Invalid filename.".into()));
            }
        }

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
        if media.results.is_empty() {
            return Err(NurError::NotFound);
        }
        let mut renamed_from = None;

        if let Some(name) = content.get("filename").and_then(|v| v.as_str())
            && let Some(m) = media.results.first_mut()
            && m.filename.as_deref() != Some(name)
        {
            let old_extension = m
                .filename
                .as_deref()
                .and_then(|filename| FsPath::new(filename).extension())
                .and_then(|extension| extension.to_str());
            let new_extension = FsPath::new(name)
                .extension()
                .and_then(|extension| extension.to_str());
            if old_extension.is_none()
                || !old_extension
                    .zip(new_extension)
                    .is_some_and(|(old, new)| old.eq_ignore_ascii_case(new))
            {
                return Err(NurError::BadRequest(
                    "Changing a media file extension is not allowed.".into(),
                ));
            }

            renamed_from = m.filename.clone();
            rename_media_file(m, name).await?;
        }

        let database_result = async {
            let mut transaction = pool.begin().await?;

            if renamed_from.is_some()
                && let Some(media) = media.results.first()
            {
                for variant in &media.variants {
                    sqlx::query("UPDATE media_variants SET filename = $1 WHERE id = $2")
                        .bind(&variant.filename)
                        .bind(variant.id)
                        .execute(&mut *transaction)
                        .await?;
                }
            }

            if let Some(filename) = content.get("filename").and_then(Value::as_str) {
                sqlx::query("UPDATE media SET filename = $1 WHERE id = $2")
                    .bind(filename)
                    .bind(id)
                    .execute(&mut *transaction)
                    .await?;
            }
            if let Some(alt) = content.get("alt") {
                sqlx::query("UPDATE media SET alt = $1 WHERE id = $2")
                    .bind(alt.as_str())
                    .bind(id)
                    .execute(&mut *transaction)
                    .await?;
            }

            transaction.commit().await?;
            Ok::<_, NurError>(())
        }
        .await;

        if let Err(error) = database_result {
            if let Some(old_filename) = renamed_from
                && let Some(media) = media.results.first_mut()
                && let Err(rollback_error) = rename_media_file(media, &old_filename).await
            {
                error!("Failed to roll back media rename: {rollback_error}");
            }
            return Err(error);
        }

        return Ok(());
    }

    Err(NurError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}
