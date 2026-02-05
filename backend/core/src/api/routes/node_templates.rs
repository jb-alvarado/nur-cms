use axum::{
    Json,
    extract::{OriginalUri, Path, Query, State},
};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::db::{
    fields::{ContentNodeTemplateFields, Table},
    handles,
    models::{ContentNodeTemplate, Role},
    queries::{QueryObj, RespondObj},
};
use crate::utils::errors::NurError;

pub async fn template_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<ContentNodeTemplateFields>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<ContentNodeTemplate>>, NurError> {
    if !details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return Err(NurError::Forbidden(
            "You do not have permission to access this resource.".into(),
        ));
    }

    params.path = original_uri.path().into();
    params.query = original_uri.query().unwrap_or("").into();

    match handles::select_record(&pool, &Table::ContentNodeTemplates, params).await {
        Ok(template) => Ok(Json(template)),
        Err(e) => {
            error!("{e}");
            Err(NurError::InternalServerError)
        }
    }
}

pub async fn template_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::delete_record(&pool, &Table::ContentNodeTemplates, id).await {
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

pub async fn template_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(template): Json<ContentNodeTemplate>,
) -> Result<Json<i32>, NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::insert_record(&pool, &Table::ContentNodeTemplates, &template).await {
            Ok(id) => Ok(Json(id)),
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

pub async fn template_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(template): Json<ContentNodeTemplate>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::update_record(&pool, &Table::ContentNodeTemplates, id, &template)
            .await
        {
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
