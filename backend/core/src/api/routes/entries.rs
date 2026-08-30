use axum::{
    Json,
    extract::{Extension, OriginalUri, Path, State},
    response::{IntoResponse, Response},
};
use axum_extra::extract::Query;
use chrono::Utc;
use markdown::{ParseOptions, to_mdast};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use serde_json::Value;
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::{
    CONFIG,
    api::entry_cache::{EntryCache, encode_json, json_response},
    db::{
        fields::{ContentEntryFields as CEF, ContentNodeFields as CNF, OutputType, Table},
        handles::{self, ContentEntryFacetQuery},
        models::{AuthUserMeta, Role},
        queries::QueryObj,
    },
    utils::{
        ast_serialize::persist_content_media_on, content_output::render_entry_nodes,
        errors::NurError,
    },
};

pub async fn entry_facets_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Extension(cache): Extension<EntryCache>,
    Query(params): Query<ContentEntryFacetQuery>,
    OriginalUri(original_uri): OriginalUri,
) -> Result<Response, NurError> {
    let cache_key = cache
        .enabled()
        .then(|| cache.entry_key(&original_uri.to_string(), "facets"));
    if let Some(response) = cache_key.as_deref().and_then(|key| cache.get(key)) {
        return Ok(json_response(response));
    }

    let facets = handles::select_content_entry_facets(&pool, &params).await?;
    if let Some(key) = cache_key {
        let response = encode_json(&facets)?;
        cache.insert(key, response.clone());
        return Ok(json_response(response));
    }

    Ok(Json(facets).into_response())
}

pub async fn entries_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Extension(cache): Extension<EntryCache>,
    Query(mut params): Query<QueryObj<CEF>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Response, NurError> {
    params.path = original_uri.path().into();
    params.query = original_uri.query().unwrap_or("").into();

    let mut output = CONFIG.read().await.output_type.clone();

    if let Some(typ) = &params.output_type
        && details.has_any_authority(&[&Role::Admin, &Role::Author])
    {
        output = typ.clone();
    }

    let is_public = !details.has_any_authority(&[&Role::Admin, &Role::Author]);
    if is_public {
        params.search_status = Some("published".to_string());
    }

    if params.fields.contains(&CEF::Node(CNF::Text))
        && !params.fields.contains(&CEF::Node(CNF::Embeds))
        && output == OutputType::AST
    {
        params.fields.push(CEF::Node(CNF::Embeds));
    }

    let cache_key = (is_public && cache.enabled())
        .then(|| cache.entry_key(&original_uri.to_string(), &format!("{output:?}")));
    if let Some(response) = cache_key.as_deref().and_then(|key| cache.get(key)) {
        return Ok(json_response(response));
    }

    let mut content = handles::select_content_entries(&pool, &params).await?;

    if params.fields.contains(&CEF::Node(CNF::Text)) {
        render_entry_nodes(&mut content.results, &output, params.character_limit)?;
    }

    if let Some(key) = cache_key {
        let response = encode_json(&content)?;
        cache.insert(key, response.clone());
        return Ok(json_response(response));
    }

    Ok(Json(content).into_response())
}

pub async fn entry_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Extension(cache): Extension<EntryCache>,
    Path((type_slug, slug)): Path<(String, String)>,
    Query(mut params): Query<QueryObj<CEF>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Response, NurError> {
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
        && output == OutputType::AST
    {
        params.fields.push(CEF::Node(CNF::Embeds));
    }

    let is_public = !details.has_any_authority(&[&Role::Admin, &Role::Author]);
    if is_public {
        params.search_status = Some("published".to_string());
    }

    let cache_key = (is_public && cache.enabled())
        .then(|| cache.entry_key(&original_uri.to_string(), &format!("{output:?}")));
    if let Some(response) = cache_key.as_deref().and_then(|key| cache.get(key)) {
        return Ok(json_response(response));
    }

    let character_limit = params.character_limit;

    if output == OutputType::AST {
        params.character_limit = None;
    }

    if let Some(mut content) = handles::select_content_entries(&pool, &params)
        .await?
        .results
        .into_iter()
        .next()
    {
        if params.fields.contains(&CEF::Node(CNF::Text)) {
            render_entry_nodes(std::slice::from_mut(&mut content), &output, character_limit)?;
        }

        if let Some(key) = cache_key {
            let response = encode_json(&content)?;
            cache.insert(key, response.clone());
            return Ok(json_response(response));
        }

        return Ok(Json(content).into_response());
    }

    Err(NurError::NotFound)
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

    let mut nodes = content.get("nodes").cloned();
    let meta = content.get("meta").cloned();

    if let Some(obj) = content.as_object_mut() {
        obj.remove("nodes");
    }

    if let Some(obj) = content.as_object_mut() {
        obj.remove("meta");
    }

    let mut transaction = pool.begin().await?;

    if let Some(nodes_arr) = nodes.as_mut().and_then(Value::as_array_mut) {
        handles::normalize_entry_node_templates(&mut transaction, nodes_arr).await?;
    }

    let id = handles::insert_entry_on(&mut transaction, &content).await?;

    if let Some(mut m) = meta {
        m["entry_id"] = Value::Number(id.into());

        let _: i32 = handles::insert_record(&mut *transaction, &Table::ContentMeta, &m).await?;
    }

    let mut order_index = 1;

    if let Some(nodes_arr) = nodes.as_ref().and_then(Value::as_array) {
        for node in nodes_arr {
            if let Some(blocks) = node.get("blocks").and_then(|b| b.as_array()) {
                let mut parent_id: Option<Value> = None;

                for block in blocks {
                    let mut block = block.clone();
                    block["entry_id"] = id.into();
                    block["order_index"] = order_index.into();

                    if let Some(obj) = block.as_object_mut() {
                        obj.remove("media");
                    }

                    if let Some(ref p_id) = parent_id {
                        block["parent_id"] = p_id.clone();
                    }

                    let block_id: i64 =
                        handles::insert_record(&mut *transaction, &Table::ContentNodes, &block)
                            .await?;

                    if parent_id.is_none() {
                        parent_id = Some(block_id.into());
                    }

                    order_index += 1;
                }
            } else {
                let mut node = node.clone();
                node["entry_id"] = id.into();
                node["order_index"] = order_index.into();

                if let Some(obj) = node.as_object_mut() {
                    obj.remove("media");
                }

                let node_id: i64 =
                    handles::insert_record(&mut *transaction, &Table::ContentNodes, &node).await?;

                if let Some(text) = node.get("text").and_then(|t| t.as_str()) {
                    let ast = to_mdast(text, &ParseOptions::gfm())?;
                    let tree: Value = serde_json::to_value(ast).unwrap_or_default();

                    persist_content_media_on(&mut transaction, node_id, &tree).await?;
                }

                order_index += 1;
            }
        }
    }

    transaction.commit().await?;

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

    handles::update_entry_with_nodes(&pool, id, &content).await?;

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
