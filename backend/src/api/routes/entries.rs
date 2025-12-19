use std::collections::HashSet;

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
    CONFIG, PUBLIC_UPLOADS,
    db::{
        fields::{ContentEntryFields, OutputType, Table},
        handles,
        models::{AuthUserMeta, ContentMedia, Role},
        queries::{QueryObj, RespondObj},
        serialize::*,
    },
    utils::{ast_serialize::to_structure_root, errors::ServiceError},
};

#[derive(Debug)]
struct AstImageRef {
    url: String,
    ast_line: i32,
    start_offset: Option<i32>,
    end_offset: Option<i32>,
}

fn collect_image_refs(node: &Value, acc: &mut Vec<AstImageRef>) {
    match node {
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("image") {
                let url = map
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();

                let position = map.get("position");
                let ast_line = position
                    .and_then(|pos| pos.get("start"))
                    .and_then(|start| start.get("line"))
                    .and_then(Value::as_i64)
                    .and_then(|v| i32::try_from(v).ok())
                    .unwrap_or_default();

                let start_offset = position
                    .and_then(|pos| pos.get("start"))
                    .and_then(|start| start.get("offset"))
                    .and_then(Value::as_i64)
                    .and_then(|v| i32::try_from(v).ok());

                let end_offset = position
                    .and_then(|pos| pos.get("end"))
                    .and_then(|end| end.get("offset"))
                    .and_then(Value::as_i64)
                    .and_then(|v| i32::try_from(v).ok());

                acc.push(AstImageRef {
                    url,
                    ast_line,
                    start_offset,
                    end_offset,
                });
            }

            if let Some(children) = map.get("children").and_then(Value::as_array) {
                for child in children {
                    collect_image_refs(child, acc);
                }
            }
        }
        Value::Array(arr) => {
            for child in arr {
                collect_image_refs(child, acc);
            }
        }
        _ => {}
    }
}

fn normalize_media_path(raw_url: &str) -> Option<(String, String)> {
    let mut path = raw_url.trim().to_string();
    if path.is_empty() {
        return None;
    }

    if let Some(pos) = path.find("://") {
        if let Some(slash_pos) = path[pos + 3..].find('/') {
            path = path[pos + 3 + slash_pos..].to_string();
        } else {
            return None;
        }
    }

    if let Some(pos) = path.find('#') {
        path.truncate(pos);
    }

    if let Some(pos) = path.find('?') {
        path.truncate(pos);
    }

    if !path.starts_with(PUBLIC_UPLOADS) {
        return None;
    }

    let (dir, filename) = path.rsplit_once('/')?;
    if filename.is_empty() {
        return None;
    }

    let dir = if dir.is_empty() {
        "/".to_string()
    } else {
        dir.to_string()
    };

    Some((dir, filename.to_string()))
}

async fn persist_content_media(
    pool: &PgPool,
    entry_id: i32,
    ast: &Value,
) -> Result<(), ServiceError> {
    let mut images = Vec::new();
    collect_image_refs(ast, &mut images);

    if images.is_empty() {
        return Ok(());
    }

    let mut seen = HashSet::new();

    for image in images {
        let Some((path, filename)) = normalize_media_path(&image.url) else {
            continue;
        };

        if let Some(media_id) = handles::select_media_id_by_path(pool, &path, &filename).await? {
            if !seen.insert((media_id, image.ast_line)) {
                continue;
            }

            let link = ContentMedia {
                entry_id,
                media_id,
                ast_line: image.ast_line,
                start_offset: image.start_offset,
                end_offset: image.end_offset,
            };

            if let Err(e) =
                handles::insert_record::<ContentMedia, i64>(pool, &Table::ContentMedia, &link).await
            {
                error!("content_media insert error: {e}");
            }
        }
    }

    Ok(())
}

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

    if !details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        params.search_status = Some("published".to_string());
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

pub async fn entry_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Extension(user): Extension<AuthUserMeta>,
    Json(mut content): Json<Value>,
) -> Result<Json<i32>, ServiceError> {
    if !details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return Err(ServiceError::Forbidden(
            "You do not have permission to access this resource.".into(),
        ));
    }

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

    let id = handles::insert_record(&pool, &Table::ContentEntries, &content).await?;
    let ast = to_mdast(
        content
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or_default(),
        &ParseOptions::default(),
    )?;
    let tree: Value = serde_json::to_value(ast).unwrap_or_default();

    persist_content_media(&pool, id, &tree).await?;

    Ok(Json(id))
}

pub async fn entry_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Extension(user): Extension<AuthUserMeta>,
    Json(mut content): Json<Value>,
) -> Result<(), ServiceError> {
    if !details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return Err(ServiceError::Forbidden(
            "You do not have permission to access this resource.".into(),
        ));
    }

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

    handles::update_record(&pool, &Table::ContentEntries, id, &content).await?;

    let text = if let Some(t) = content.get("text").and_then(|t| t.as_str()) {
        t.to_owned()
    } else {
        handles::select_entry_text(&pool, id)
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
    };

    let ast = to_mdast(&text, &ParseOptions::default())?;
    let tree: Value = serde_json::to_value(ast).unwrap_or_default();

    handles::delete_content_media_for_entry(&pool, id).await?;
    persist_content_media(&pool, id, &tree).await?;

    Ok(())
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
