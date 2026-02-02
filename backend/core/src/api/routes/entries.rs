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
        fields::{ContentEntryFields as CEF, ContentNodeFields as CNF, OutputType, Table},
        handles,
        models::{AuthUserMeta, Role},
        queries::{QueryObj, RespondObj},
        serialize::*,
    },
    utils::{
        ast_serialize::{persist_content_media, to_structure_root},
        errors::NurError,
    },
};

pub async fn entries_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<CEF>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<ContentEntrySerializer>>, NurError> {
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

    if params.fields.contains(&CEF::Node(CNF::Text))
        && !params.fields.contains(&CEF::Node(CNF::Embeds))
        && params.output_type == Some(OutputType::AST)
        && params.character_limit.is_none()
    {
        params.fields.push(CEF::Node(CNF::Embeds));
    }

    let mut content = handles::select_content_entries(&pool, &params).await?;

    if params.fields.contains(&CEF::Node(CNF::Text)) && output != OutputType::Markdown {
        for entry in &mut content.results {
            for node_wrapper in &mut entry.nodes {
                // Extract the nodes based on Single or Block variant
                let nodes_to_process: Vec<&mut ContentNodeSerializer> = match node_wrapper {
                    NodeSerializer::Single(node) => vec![node.as_mut()],
                    NodeSerializer::Blocks(nodes) => nodes.iter_mut().collect(),
                };

                for node in nodes_to_process {
                    let text = node.text.take().unwrap_or_default();
                    node.text = None;

                    if !text.is_empty() {
                        match output {
                            OutputType::AST => {
                                let ast = to_mdast(&text, &ParseOptions::default())?;
                                let json = serde_json::to_string(&ast).unwrap_or_default();
                                let tree: Value = serde_json::from_str(&json).unwrap_or_default();
                                let body = to_structure_root(&tree, &mut node.embeds);

                                node.ast = Some(body);
                            }
                            OutputType::HTML => {
                                let html = to_html(&text);
                                node.html = Some(html);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    Ok(Json(content))
}

pub async fn entry_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path((type_slug, slug)): Path<(String, String)>,
    Query(mut params): Query<QueryObj<CEF>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<ContentEntrySerializer>, NurError> {
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

    if params.fields.contains(&CEF::Node(CNF::Text))
        && !params.fields.contains(&CEF::Node(CNF::Embeds))
        && params.output_type == Some(OutputType::AST)
        && params.character_limit.is_none()
    {
        params.fields.push(CEF::Node(CNF::Embeds));
    }

    if let Some(mut content) = handles::select_content_entries(&pool, &params)
        .await?
        .results
        .into_iter()
        .next()
    {
        if params.fields.contains(&CEF::Node(CNF::Text)) && output != OutputType::Markdown {
            for node_wrapper in &mut content.nodes {
                // Extract the nodes based on Single or Block variant
                let nodes_to_process: Vec<&mut ContentNodeSerializer> = match node_wrapper {
                    NodeSerializer::Single(node) => vec![node.as_mut()],
                    NodeSerializer::Blocks(nodes) => nodes.iter_mut().collect(),
                };

                for node in nodes_to_process {
                    let text = node.text.take().unwrap_or_default();
                    node.text = None;

                    if !text.is_empty() {
                        match output {
                            OutputType::AST => {
                                let ast = to_mdast(&text, &ParseOptions::default())?;
                                let json = serde_json::to_string(&ast).unwrap_or_default();
                                let tree: Value = serde_json::from_str(&json).unwrap_or_default();
                                let body = to_structure_root(&tree, &mut node.embeds);

                                node.ast = Some(body);
                            }
                            OutputType::HTML => {
                                let html = to_html(&text);
                                node.html = Some(html);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        return Ok(Json(content));
    }

    Err(NurError::NoContent)
}

pub async fn entry_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Extension(user): Extension<AuthUserMeta>,
    Json(mut content): Json<Value>,
) -> Result<Json<i32>, NurError> {
    if !details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return Err(NurError::Forbidden(
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

    let meta = content.get("meta").cloned();
    if let Some(obj) = content.as_object_mut() {
        obj.remove("meta");
    }

    let id: i32 = handles::insert_record(&pool, &Table::ContentEntries, &content).await?;

    if let Some(mut m) = meta {
        m["entry_id"] = Value::Number(id.into());

        let _: i32 = handles::insert_record(&pool, &Table::ContentMeta, &m).await?;
    }

    let ast = to_mdast(
        content
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or_default(),
        &ParseOptions::default(),
    )?;
    let tree: Value = serde_json::to_value(ast).unwrap_or_default();

    persist_content_media(&pool, id.into(), &tree).await?;

    // for block in blocks {
    //     let mut block = block.clone();
    //     block["entry_id"] = id.into();

    //     let _: i32 = handles::insert_record(&pool, &Table::ContentBlocks, &block).await?;
    // }

    Ok(Json(id))
}

pub async fn entry_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Extension(user): Extension<AuthUserMeta>,
    Json(mut content): Json<Value>,
) -> Result<(), NurError> {
    if !details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return Err(NurError::Forbidden(
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

    handles::update_entry_with_blocks(&pool, id, &content).await?;

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
    persist_content_media(&pool, id.into(), &tree).await?;

    Ok(())
}

pub async fn entry_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::delete_record(&pool, &Table::ContentEntries, id).await {
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
