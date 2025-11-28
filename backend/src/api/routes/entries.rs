use axum::{
    Json,
    extract::{Extension, OriginalUri, Path, Query, State},
};
use chrono::Utc;
use markdown::{ParseOptions, to_html, to_mdast};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use serde_json::Value;
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::{
    CONFIG,
    db::{
        fields::{ContentEntryFields, OutputType, Table},
        handles,
        models::{AuthUserMeta, Role},
        queries::{QueryObj, RespondObj},
        serialize::*,
    },
    utils::{ast_serialize::to_structure_root, errors::ServiceError},
};

pub async fn entries_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<ContentEntryFields>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<ContentEntrySerializer>>, ServiceError> {
    params.path = original_uri.path().into();
    params.query = original_uri.query().unwrap_or("").into();

    let mut output = CONFIG.read().await.output_type.clone();

    if let Some(typ) = &params.output_type
        && details.has_any_authority(&[&Role::Admin, &Role::Author])
    {
        output = typ.clone();
    }

    let mut content = handles::select_content_entries(&pool, &params).await?;

    if params.fields.contains(&ContentEntryFields::Body) && output != OutputType::Markdown {
        for b in &mut content.results {
            let text = b.text.take().unwrap_or_default();
            b.text = None;

            match output {
                OutputType::AST => {
                    let ast = to_mdast(&text, &ParseOptions::default())?;
                    let json = serde_json::to_string(&ast).unwrap_or_default();
                    let tree: Value = serde_json::from_str(&json).unwrap_or_default();
                    let body = to_structure_root(&tree, &mut b.embeds);

                    b.ast = Some(body);
                }
                OutputType::HTML => {
                    let html = to_html(&text);
                    b.html = Some(html);
                }
                _ => {}
            }
        }
    }

    Ok(Json(content))
}

pub async fn entry_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path((type_slug, slug)): Path<(String, String)>,
    Query(mut params): Query<QueryObj<ContentEntryFields>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<ContentEntrySerializer>, ServiceError> {
    params.path = original_uri.path().into();
    params.query = original_uri.query().unwrap_or("").into();
    params.type_slug = Some(type_slug);
    params.search_slug = Some(slug);

    let mut output = CONFIG.read().await.output_type.clone();

    if let Some(typ) = &params.output_type
        && details.has_any_authority(&[&Role::Admin, &Role::Author])
    {
        output = typ.clone();
    }

    if let Some(mut content) = handles::select_content_entries(&pool, &params)
        .await?
        .results
        .into_iter()
        .next()
    {
        if params.fields.contains(&ContentEntryFields::Body) && output != OutputType::Markdown {
            let text = content.text.take().unwrap_or_default();

            match output {
                OutputType::AST => {
                    let ast = to_mdast(&text, &ParseOptions::default())?;
                    let tree: Value = serde_json::to_value(ast).unwrap_or_default();
                    let body = to_structure_root(&tree, &mut content.embeds);
                    content.ast = Some(body);
                }
                OutputType::HTML => {
                    let html = to_html(&text);
                    content.html = Some(html);
                }
                _ => {}
            }
        }

        return Ok(Json(content));
    }

    Err(ServiceError::NoContent)
}

pub async fn entry_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Extension(user): Extension<AuthUserMeta>,
    Json(mut content): Json<Value>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        content["updated_at"] = Value::String(Utc::now().to_rfc3339());
        content["updated_by"] = user.id.into();

        if let Some(body) = content.get("body")
            && content.get("text").is_none()
        {
            content["text"] = body.clone();
        }

        if let Some(obj) = content.as_object_mut() {
            obj.remove("body");
        }

        return match handles::update_record(&pool, &Table::ContentEntries, id, &content).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::Conflict(e.to_string()))
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}

pub async fn entry_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::delete_record(&pool, &Table::ContentEntries, id).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}

pub async fn entry_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Extension(user): Extension<AuthUserMeta>,
    Json(mut content): Json<Value>,
) -> Result<Json<i32>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        content["created_by"] = user.id.into();
        content["updated_by"] = user.id.into();

        if let Some(body) = content.get("body")
            && content.get("text").is_none()
        {
            content["text"] = body.clone();
        }

        if let Some(obj) = content.as_object_mut() {
            obj.remove("body");
        }

        return match handles::insert_record(&pool, &Table::ContentEntries, &content).await {
            Ok(id) => Ok(Json(id)),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::Conflict(e.to_string()))
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}
