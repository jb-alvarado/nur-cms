use axum::{
    Json,
    extract::{Extension, OriginalUri, Path, State},
    response::{IntoResponse, Response},
};
use axum_extra::extract::Query;
use chrono::Utc;
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
    utils::{content_output::render_entry_nodes, errors::NurError},
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

    let configuration = CONFIG.read().await;
    let mut output = configuration.output_type.clone();
    let max_image_variant_width = configuration.max_image_resolution();
    drop(configuration);

    if let Some(typ) = &params.output_type
        && (details.has_any_authority(&[&Role::Admin, &Role::Author]) || cfg!(debug_assertions))
    {
        output = typ.clone();
    }

    let is_public = !details.has_any_authority(&[&Role::Admin, &Role::Author]);
    if is_public {
        params.search_status = Some("published".to_string());
    }

    let embeds_requested = params.fields.contains(&CEF::Node(CNF::Embeds));
    if params.fields.contains(&CEF::Node(CNF::Text))
        && !embeds_requested
        && matches!(output, OutputType::AST | OutputType::HTML)
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
        render_entry_nodes(
            &mut content.results,
            &output,
            params.character_limit,
            embeds_requested,
            max_image_variant_width,
        )?;
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

    let configuration = CONFIG.read().await;
    let mut output = configuration.output_type.clone();
    let max_image_variant_width = configuration.max_image_resolution();
    drop(configuration);

    if let Some(typ) = &params.output_type
        && (details.has_any_authority(&[&Role::Admin, &Role::Author]) || cfg!(debug_assertions))
    {
        output = typ.clone();
    }

    let embeds_requested = params.fields.contains(&CEF::Node(CNF::Embeds));
    if params.fields.contains(&CEF::Node(CNF::Text))
        && !embeds_requested
        && matches!(output, OutputType::AST | OutputType::HTML)
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
            render_entry_nodes(
                std::slice::from_mut(&mut content),
                &output,
                character_limit,
                embeds_requested,
                max_image_variant_width,
            )?;
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

    Ok(Json(
        handles::insert_entry_with_nodes(&pool, &content).await?,
    ))
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
