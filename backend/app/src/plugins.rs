use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{DefaultBodyLimit, Extension, Multipart, Path, Query, Request, State},
    http::{HeaderName, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{MethodFilter, get, on, post},
};
use bytes::Bytes;
use moka::sync::Cache;
use nur_core::{
    MAX_CHUNK_SIZE, MAX_UPLOAD_SIZE,
    db::models::{AuthUserMeta, Role},
    file::helper::{
        Upload, cleanup_stale_uploads, cleanup_upload, cleanup_upload_for_output,
        get_or_create_preallocated_upload_for_owner, received_ranges, reset_finalizing,
        valid_upload_session_id, write_upload_chunk,
    },
};
use nur_plugins::{
    BrowserUpload, CachePolicy, Error, Header, Identity, PluginManager, Request as PluginRequest,
    Response as PluginResponse, Route,
};
use protect_axum::authorities::AuthDetails;
use real::RealIp;
use serde::{Deserialize, Serialize};
use tokio_util::io::ReaderStream;
use tower_http::{services::ServeDir, timeout::TimeoutLayer};
use tracing::{error, info};

const FORWARDED_REQUEST_HEADERS: &[&str] =
    &["accept", "accept-language", "content-type", "user-agent"];

#[derive(Deserialize)]
struct StorageUploadQuery {
    filename: String,
}

#[derive(Deserialize)]
struct StoragePathQuery {
    path: String,
}

#[derive(Deserialize)]
struct LinkUploadStatusQuery {
    file_name: Option<String>,
    size: u64,
    batch_id: String,
}

#[derive(Serialize)]
struct LinkUploadStatus {
    received_ranges: Vec<(u64, u64)>,
    complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<nur_plugins::StoredFile>,
}

enum LinkUploadReservation {
    Pending(ReservedLinkUpload),
    Complete(nur_plugins::StoredFile),
}

struct ReservedLinkUpload {
    link_id: i64,
    directory: nur_plugins::StorageDirectory,
    filename: String,
    total_size: u64,
    output_file: std::path::PathBuf,
    upload: Upload,
}

struct UploadChunk {
    file_name: String,
    start: u64,
    end: u64,
    size: u64,
    data: Vec<u8>,
    batch_id: String,
}

struct FileRouteError(Box<Response>);

impl FileRouteError {
    fn new(response: Response) -> Self {
        Self(Box::new(response))
    }
}

impl From<StatusCode> for FileRouteError {
    fn from(status: StatusCode) -> Self {
        Self::new(status.into_response())
    }
}

impl IntoResponse for FileRouteError {
    fn into_response(self) -> Response {
        *self.0
    }
}

#[derive(Clone)]
pub struct PluginCacheInvalidator {
    caches: Arc<Vec<RouteCache>>,
}
impl PluginCacheInvalidator {
    pub fn invalidate(&self) {
        for cache in self.caches.iter() {
            cache.invalidate();
        }
    }
}

#[derive(Clone)]
struct RouteCache {
    responses: Cache<String, CachedResponse>,
    ttl: Duration,
    generation: Arc<AtomicU64>,
}
#[derive(Clone)]
struct CachedResponse {
    status: u16,
    headers: Vec<Header>,
    body: Bytes,
    expires_at: Instant,
}
#[derive(Clone)]
struct RouteState {
    manager: Arc<PluginManager>,
    route: Route,
    cache: Option<RouteCache>,
    plugin_cache: Option<RouteCache>,
}

#[derive(Clone)]
struct FileRouteState {
    manager: Arc<PluginManager>,
    invalidator: PluginCacheInvalidator,
}

pub struct PluginRouter {
    pub router: Router,
    invalidator: PluginCacheInvalidator,
}
impl PluginRouter {
    pub fn cache_invalidator(&self) -> PluginCacheInvalidator {
        self.invalidator.clone()
    }
}

pub fn router(manager: Arc<PluginManager>) -> Result<PluginRouter, Error> {
    for plugin in manager.metadata() {
        info!(plugin = %plugin.id, version = %plugin.version, "loaded plugin");
    }
    let routes = manager.routes();
    let configured = routes
        .iter()
        .filter(|route| route.cache.is_some())
        .map(|route| route.plugin_id.as_str())
        .collect::<HashSet<_>>()
        .len();
    let capacity = cache_capacity(configured);
    let mut caches = HashMap::<String, RouteCache>::new();
    for route in &routes {
        if let Some(policy) = route.cache {
            caches
                .entry(route.plugin_id.clone())
                .or_insert_with(|| route_cache(policy, capacity));
        }
    }
    let invalidator = PluginCacheInvalidator {
        caches: Arc::new(caches.values().cloned().collect()),
    };
    let mut router =
        Router::new().route("/api/plugins", get(index).with_state(Arc::clone(&manager)));
    if manager.storage().is_some() {
        let file_state = FileRouteState {
            manager: Arc::clone(&manager),
            invalidator: invalidator.clone(),
        };
        let link_upload_router = Router::new()
            .route(
                "/api/plugins/{plugin}/files/upload/{token}",
                get(link_upload_status).post(link_upload_chunk),
            )
            .layer(DefaultBodyLimit::max(
                usize::try_from(*MAX_CHUNK_SIZE)
                    .unwrap_or(10 * 1024 * 1024)
                    .saturating_add(64 * 1024),
            ));
        let file_router = Router::new()
            .merge(link_upload_router)
            .route(
                "/api/plugins/{plugin}/files/download/{token}",
                get(link_download),
            )
            .route(
                "/api/plugins/{plugin}/files/{directory}",
                post(authenticated_upload).delete(authenticated_delete),
            )
            .route(
                "/api/plugins/{plugin}/files/{directory}/download",
                get(authenticated_download),
            )
            .with_state(file_state);
        router = router.merge(file_router.layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(300),
        )));
    }
    for asset in manager.assets() {
        router = router.nest_service(
            &format!("/plugins/{}/assets", asset.plugin_id),
            ServeDir::new(&asset.path),
        );
    }
    let mut runtime_router = Router::new();
    for route in routes {
        let method = method_filter(&route.method)?;
        let state = Arc::new(RouteState {
            cache: route
                .cache
                .and_then(|_| caches.get(&route.plugin_id).cloned()),
            plugin_cache: caches.get(&route.plugin_id).cloned(),
            manager: Arc::clone(&manager),
            route,
        });
        runtime_router = runtime_router.merge(
            Router::new()
                .route(&state.route.path, on(method, dispatch))
                .with_state(state),
        );
    }
    router = router.merge(runtime_router.layer(TimeoutLayer::with_status_code(
        StatusCode::GATEWAY_TIMEOUT,
        manager.timeout() + Duration::from_millis(250),
    )));
    Ok(PluginRouter {
        router,
        invalidator,
    })
}

async fn index(State(manager): State<Arc<PluginManager>>, details: AuthDetails<Role>) -> Response {
    let roles = role_names(&details);
    if roles.is_empty() {
        return StatusCode::FORBIDDEN.into_response();
    }
    axum::Json(manager.visible_metadata(&roles)).into_response()
}

async fn link_upload_status(
    State(state): State<FileRouteState>,
    Path((plugin_id, token)): Path<(String, String)>,
    Query(query): Query<LinkUploadStatusQuery>,
) -> Response {
    let reserved = match reserve_link_upload(
        &state,
        &plugin_id,
        &token,
        &query.batch_id,
        query.size,
        query.file_name.as_deref(),
    )
    .await
    {
        Ok(LinkUploadReservation::Pending(reserved)) => reserved,
        Ok(LinkUploadReservation::Complete(file)) => {
            return axum::Json(LinkUploadStatus {
                received_ranges: Vec::new(),
                complete: true,
                file: Some(file),
            })
            .into_response();
        }
        Err(error) => return error.into_response(),
    };
    axum::Json(LinkUploadStatus {
        received_ranges: received_ranges(&reserved.upload).await,
        complete: false,
        file: None,
    })
    .into_response()
}

async fn link_upload_chunk(
    State(state): State<FileRouteState>,
    Path((plugin_id, token)): Path<(String, String)>,
    multipart: Multipart,
) -> Response {
    let chunk = match parse_upload_chunk(multipart).await {
        Ok(chunk) => chunk,
        Err(error) => return error.into_response(),
    };
    let reserved = match reserve_link_upload(
        &state,
        &plugin_id,
        &token,
        &chunk.batch_id,
        chunk.size,
        Some(&chunk.file_name),
    )
    .await
    {
        Ok(LinkUploadReservation::Pending(reserved)) => reserved,
        Ok(LinkUploadReservation::Complete(file)) => return axum::Json(file).into_response(),
        Err(error) => return error.into_response(),
    };
    let should_finalize =
        match write_upload_chunk(&reserved.upload, chunk.start, chunk.end, &chunk.data).await {
            Ok(value) => value,
            Err(error) => return sanitized_upload_error(error).into_response(),
        };
    if !should_finalize {
        return axum::Json(LinkUploadStatus {
            received_ranges: received_ranges(&reserved.upload).await,
            complete: false,
            file: None,
        })
        .into_response();
    }

    if let Err(error) = state
        .manager
        .begin_upload_link_finalization(reserved.link_id, &chunk.batch_id)
        .await
    {
        reset_finalizing(&reserved.upload).await;
        return file_link_error(error);
    }
    let Some(storage) = state.manager.storage().cloned() else {
        reset_finalizing(&reserved.upload).await;
        return StatusCode::NOT_FOUND.into_response();
    };
    let directory = reserved.directory.clone();
    let filename = reserved.filename.clone();
    let temporary = reserved.upload.temp_file.clone();
    let quota = state.manager.storage_quota();
    let finalized = tokio::task::spawn_blocking(move || {
        storage.complete_resumable_upload(
            &directory,
            &filename,
            &temporary,
            reserved.total_size,
            quota,
        )
    })
    .await;
    let file = match finalized {
        Ok(Ok(file)) => file,
        Ok(Err(error)) => {
            reset_finalizing(&reserved.upload).await;
            return plugin_storage_error(error, "plugin link upload finalization failed");
        }
        Err(error) => {
            reset_finalizing(&reserved.upload).await;
            error!(%error, "plugin link upload finalization task failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if let Err(error) = state
        .manager
        .complete_upload_link(reserved.link_id, &chunk.batch_id)
        .await
    {
        reset_finalizing(&reserved.upload).await;
        return file_link_error(error);
    }
    cleanup_upload(&reserved.output_file, &reserved.upload).await;
    state.invalidator.invalidate();
    axum::Json(file).into_response()
}

async fn reserve_link_upload(
    state: &FileRouteState,
    plugin_id: &str,
    token: &str,
    batch_id: &str,
    total_size: u64,
    supplied_filename: Option<&str>,
) -> Result<LinkUploadReservation, FileRouteError> {
    if !valid_upload_session_id(batch_id) || total_size == 0 || total_size > *MAX_UPLOAD_SIZE {
        return Err(StatusCode::BAD_REQUEST.into());
    }
    let (link_id, directory, filename, link_limit, finalizing) = state
        .manager
        .reserve_upload_link(plugin_id, token, batch_id, total_size)
        .await
        .map_err(|error| FileRouteError::new(file_link_error(error)))?;
    if supplied_filename.is_some_and(|supplied| supplied != filename) {
        return Err(StatusCode::BAD_REQUEST.into());
    }
    let effective_limit = link_limit.min(*MAX_UPLOAD_SIZE);
    if total_size > effective_limit {
        return Err(StatusCode::PAYLOAD_TOO_LARGE.into());
    }
    let storage = state
        .manager
        .storage()
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    if finalizing {
        let recovery_storage = storage.clone();
        let recovery_directory = directory.clone();
        let recovery_filename = filename.clone();
        let recovered = tokio::task::spawn_blocking(move || {
            recovery_storage.recover_resumable_upload(
                &recovery_directory,
                &recovery_filename,
                total_size,
            )
        })
        .await
        .map_err(|error| {
            error!(%error, "plugin link upload recovery task failed");
            FileRouteError::from(StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .map_err(|error| {
            FileRouteError::new(plugin_storage_error(
                error,
                "plugin link upload recovery failed",
            ))
        })?;
        if let Some((file, output_file)) = recovered {
            state
                .manager
                .complete_upload_link(link_id, batch_id)
                .await
                .map_err(|error| FileRouteError::new(file_link_error(error)))?;
            cleanup_upload_for_output(&output_file).await;
            state.invalidator.invalidate();
            return Ok(LinkUploadReservation::Complete(file));
        }
    }
    let prepared_directory = directory.clone();
    let prepared_filename = filename.clone();
    let quota = state.manager.storage_quota();
    let output_file = tokio::task::spawn_blocking(move || {
        storage.prepare_resumable_upload(
            &prepared_directory,
            &prepared_filename,
            total_size,
            effective_limit,
            quota,
        )
    })
    .await
    .map_err(|error| {
        error!(%error, "plugin link upload preparation task failed");
        FileRouteError::from(StatusCode::INTERNAL_SERVER_ERROR)
    })?
    .map_err(|error| {
        FileRouteError::new(plugin_storage_error(
            error,
            "plugin link upload preparation failed",
        ))
    })?;

    cleanup_stale_uploads().await;
    let owner_id = format!("plugin:{plugin_id}");
    let upload =
        get_or_create_preallocated_upload_for_owner(total_size, &output_file, batch_id, &owner_id)
            .await
            .map_err(sanitized_upload_error)?;
    Ok(LinkUploadReservation::Pending(ReservedLinkUpload {
        link_id,
        directory,
        filename,
        total_size,
        output_file,
        upload,
    }))
}

async fn parse_upload_chunk(mut multipart: Multipart) -> Result<UploadChunk, FileRouteError> {
    let mut file_name = None;
    let mut start = None;
    let mut end = None;
    let mut size = None;
    let mut data = None;
    let mut batch_id = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| FileRouteError::from(StatusCode::BAD_REQUEST))?
    {
        match field.name().unwrap_or_default() {
            "fileName" if file_name.is_none() => {
                file_name = Some(
                    field
                        .text()
                        .await
                        .map_err(|_| FileRouteError::from(StatusCode::BAD_REQUEST))?,
                );
            }
            "start" if start.is_none() => {
                start = Some(parse_u64_field(field).await?);
            }
            "end" if end.is_none() => {
                end = Some(parse_u64_field(field).await?);
            }
            "size" if size.is_none() => {
                size = Some(parse_u64_field(field).await?);
            }
            "chunk" if data.is_none() => {
                data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|_| FileRouteError::from(StatusCode::BAD_REQUEST))?
                        .to_vec(),
                );
            }
            "batch_id" if batch_id.is_none() => {
                batch_id = Some(
                    field
                        .text()
                        .await
                        .map_err(|_| FileRouteError::from(StatusCode::BAD_REQUEST))?,
                );
            }
            "fileName" | "start" | "end" | "size" | "chunk" | "batch_id" => {
                return Err(StatusCode::BAD_REQUEST.into());
            }
            _ => {}
        }
    }

    let chunk = UploadChunk {
        file_name: file_name.ok_or(StatusCode::BAD_REQUEST)?,
        start: start.ok_or(StatusCode::BAD_REQUEST)?,
        end: end.ok_or(StatusCode::BAD_REQUEST)?,
        size: size.ok_or(StatusCode::BAD_REQUEST)?,
        data: data.ok_or(StatusCode::BAD_REQUEST)?,
        batch_id: batch_id.ok_or(StatusCode::BAD_REQUEST)?,
    };
    if !valid_upload_chunk_range(chunk.start, chunk.end, chunk.size, chunk.data.len() as u64) {
        return Err(StatusCode::BAD_REQUEST.into());
    }
    Ok(chunk)
}

fn valid_upload_chunk_range(start: u64, end: u64, size: u64, chunk_size: u64) -> bool {
    end > start && end <= size && chunk_size == end - start && chunk_size <= *MAX_CHUNK_SIZE
}

async fn parse_u64_field(
    field: axum::extract::multipart::Field<'_>,
) -> Result<u64, FileRouteError> {
    field
        .text()
        .await
        .map_err(|_| FileRouteError::from(StatusCode::BAD_REQUEST))?
        .parse()
        .map_err(|_| StatusCode::BAD_REQUEST.into())
}

fn plugin_storage_error(error: Error, message: &'static str) -> Response {
    match error {
        Error::PluginBadRequest(_) => StatusCode::BAD_REQUEST.into_response(),
        Error::PluginNotFound => StatusCode::NOT_FOUND.into_response(),
        error => {
            error!(%error, "{message}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn sanitized_upload_error(error: nur_core::utils::errors::NurError) -> FileRouteError {
    let status = error.into_response().status();
    FileRouteError::from(status)
}

async fn authenticated_upload(
    State(state): State<FileRouteState>,
    Path((plugin_id, directory_id)): Path<(String, String)>,
    Query(query): Query<StorageUploadQuery>,
    details: AuthDetails<Role>,
    request: Request,
) -> Response {
    let Some(directory) = state.manager.storage_directory(&plugin_id, &directory_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if directory.upload != BrowserUpload::Authenticated
        || !role_names(&details)
            .iter()
            .any(|role| directory.roles.contains(role))
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let quota = state.manager.storage_quota();
    let limit = usize::try_from((*MAX_UPLOAD_SIZE).min(quota))
        .unwrap_or(usize::MAX)
        .min(state.manager.storage_write_limit());
    let body = match to_bytes(request.into_body(), limit).await {
        Ok(body) if !body.is_empty() => body,
        Ok(_) => return StatusCode::BAD_REQUEST.into_response(),
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let Some(storage) = state.manager.storage().cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match tokio::task::spawn_blocking(move || {
        storage.write(&directory, &query.filename, &body, limit, quota)
    })
    .await
    {
        Ok(Ok(file)) => {
            state.invalidator.invalidate();
            axum::Json(file).into_response()
        }
        Ok(Err(Error::PluginBadRequest(_))) => StatusCode::BAD_REQUEST.into_response(),
        Ok(Err(error)) => {
            error!(%error, "authenticated plugin upload failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        Err(error) => {
            error!(%error, "authenticated plugin upload task failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn authenticated_download(
    State(state): State<FileRouteState>,
    Path((plugin_id, directory_id)): Path<(String, String)>,
    Query(query): Query<StoragePathQuery>,
    details: AuthDetails<Role>,
) -> Response {
    let directory =
        match authorized_storage_directory(&state.manager, &plugin_id, &directory_id, &details) {
            Ok(directory) => directory,
            Err(status) => return status.into_response(),
        };
    let Some(storage) = state.manager.storage().cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match stream_stored_file(storage, directory, query.path).await {
        Ok(response) => response,
        Err(Error::PluginNotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(Error::PluginBadRequest(_)) => StatusCode::BAD_REQUEST.into_response(),
        Err(error) => {
            error!(%error, "authenticated plugin download failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn authenticated_delete(
    State(state): State<FileRouteState>,
    Path((plugin_id, directory_id)): Path<(String, String)>,
    Query(query): Query<StoragePathQuery>,
    details: AuthDetails<Role>,
) -> Response {
    let directory =
        match authorized_storage_directory(&state.manager, &plugin_id, &directory_id, &details) {
            Ok(directory) => directory,
            Err(status) => return status.into_response(),
        };
    let Some(storage) = state.manager.storage().cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match tokio::task::spawn_blocking(move || storage.delete(&directory, &query.path)).await {
        Ok(Ok(())) => {
            state.invalidator.invalidate();
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(Err(Error::PluginNotFound)) => StatusCode::NOT_FOUND.into_response(),
        Ok(Err(Error::PluginBadRequest(_))) => StatusCode::BAD_REQUEST.into_response(),
        Ok(Err(error)) => {
            error!(%error, "authenticated plugin deletion failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        Err(error) => {
            error!(%error, "authenticated plugin deletion task failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn authorized_storage_directory(
    manager: &PluginManager,
    plugin_id: &str,
    directory_id: &str,
    details: &AuthDetails<Role>,
) -> Result<nur_plugins::StorageDirectory, StatusCode> {
    let directory = manager
        .storage_directory(plugin_id, directory_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    role_names(details)
        .iter()
        .any(|role| directory.roles.contains(role))
        .then_some(directory)
        .ok_or(StatusCode::FORBIDDEN)
}

async fn link_download(
    State(state): State<FileRouteState>,
    Path((plugin_id, token)): Path<(String, String)>,
) -> Response {
    let (link_id, directory, path) = match state.manager.download_link(&plugin_id, &token).await {
        Ok(link) => link,
        Err(error) => return file_link_error(error),
    };
    let Some(storage) = state.manager.storage().cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let file = match open_stored_file(storage, directory, path).await {
        Ok(file) => file,
        Err(Error::PluginNotFound | Error::PluginBadRequest(_)) => {
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(error) => {
            error!(%error, "plugin link download failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if let Err(error) = state.manager.consume_download_link(link_id).await {
        return file_link_error(error);
    }
    stored_file_response(file)
}

async fn stream_stored_file(
    storage: nur_plugins::PluginStorage,
    directory: nur_plugins::StorageDirectory,
    path: String,
) -> Result<Response, Error> {
    open_stored_file(storage, directory, path)
        .await
        .map(stored_file_response)
}

async fn open_stored_file(
    storage: nur_plugins::PluginStorage,
    directory: nur_plugins::StorageDirectory,
    path: String,
) -> Result<tokio::fs::File, Error> {
    let target = tokio::task::spawn_blocking(move || storage.file_path(&directory, &path))
        .await
        .map_err(Error::Join)??;
    tokio::fs::File::open(target).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            Error::PluginNotFound
        } else {
            Error::Io(error)
        }
    })
}

fn stored_file_response(file: tokio::fs::File) -> Response {
    Response::builder()
        .header("content-type", "application/octet-stream")
        .header("content-disposition", "attachment")
        .body(Body::from_stream(ReaderStream::new(file)))
        .unwrap_or_else(|error| {
            error!(%error, "failed to build plugin file response");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })
}

fn file_link_error(error: Error) -> Response {
    match error {
        Error::PluginNotFound => StatusCode::NOT_FOUND.into_response(),
        error => {
            error!(%error, "plugin file link lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn dispatch(
    State(state): State<Arc<RouteState>>,
    details: AuthDetails<Role>,
    Extension(user): Extension<AuthUserMeta>,
    Extension(real_ip): Extension<RealIp>,
    path_params: Option<Path<HashMap<String, String>>>,
    request: Request,
) -> Response {
    let roles = role_names(&details);
    let identity = request_identity(!state.route.roles.is_empty(), user.id, roles);
    if !state.manager.authorized(&state.route, identity.as_ref()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let request = match into_plugin_request(
        request,
        path_params,
        identity,
        real_ip.ip(),
        state.manager.request_body_limit(),
    )
    .await
    {
        Ok(request) => request,
        Err(status) => return status.into_response(),
    };
    if let Err(status) = validate_cached_request_body(state.cache.is_some(), &request.body) {
        return status.into_response();
    }
    let key = state
        .cache
        .as_ref()
        .map(|cache| cache.key(cache_key(&state.route.id, &request)));
    if let (Some(cache), Some(key)) = (&state.cache, &key)
        && let Some(cached) = cache.responses.get(key)
    {
        if cached.expires_at > Instant::now() {
            return into_cached_response(cached);
        }
        cache.responses.invalidate(key);
    }
    let is_write = is_plugin_write_method(&request.method);
    let result = state.manager.dispatch(&state.route, request).await;
    if is_write && let Some(cache) = &state.plugin_cache {
        cache.invalidate();
    }
    match result {
        Ok(response) => {
            if let (Some(cache), Some(key)) = (&state.cache, key)
                && response.status == 200
            {
                let body = Bytes::from(response.body);
                cache.responses.insert(
                    key,
                    CachedResponse {
                        status: response.status,
                        headers: response.headers.clone(),
                        body: body.clone(),
                        expires_at: Instant::now() + cache.ttl,
                    },
                );
                return into_response_parts(response.status, response.headers, body);
            }
            into_response(response)
        }
        Err(error) => {
            error!(plugin = %state.route.plugin_id, route = %state.route.id, %error, "plugin request failed");
            error_response(error)
        }
    }
}

async fn into_plugin_request(
    request: Request,
    path_params: Option<Path<HashMap<String, String>>>,
    identity: Option<Identity>,
    client_ip: std::net::IpAddr,
    body_limit: usize,
) -> Result<PluginRequest, StatusCode> {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let query = request.uri().query().map(ToOwned::to_owned);
    let headers = request_headers(request.headers());
    let body = to_bytes(request.into_body(), body_limit)
        .await
        .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?
        .to_vec();
    let params = plugin_path_params(path_params);
    Ok(PluginRequest {
        method,
        path,
        path_params: params,
        query,
        headers,
        body,
        client_ip: Some(client_ip),
        identity,
    })
}

fn role_names(details: &AuthDetails<Role>) -> Vec<String> {
    details
        .authorities
        .iter()
        .filter(|role| !matches!(role, Role::Guest))
        .map(ToString::to_string)
        .collect()
}
fn request_identity(protected: bool, user_id: i32, roles: Vec<String>) -> Option<Identity> {
    (protected && user_id >= 0).then_some(Identity { user_id, roles })
}
fn plugin_path_params(path_params: Option<Path<HashMap<String, String>>>) -> Vec<(String, String)> {
    let mut params: Vec<_> = path_params
        .unwrap_or(Path(HashMap::new()))
        .0
        .into_iter()
        .collect();
    params.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    params
}
fn validate_cached_request_body(cache_enabled: bool, body: &[u8]) -> Result<(), StatusCode> {
    if cache_enabled && !body.is_empty() {
        Err(StatusCode::BAD_REQUEST)
    } else {
        Ok(())
    }
}
fn is_plugin_write_method(method: &str) -> bool {
    matches!(method, "POST" | "PUT" | "PATCH" | "DELETE")
}
fn request_headers(headers: &axum::http::HeaderMap) -> Vec<Header> {
    headers
        .iter()
        .filter(|(name, _)| FORWARDED_REQUEST_HEADERS.contains(&name.as_str()))
        .take(32)
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .filter(|value| value.len() <= 8 * 1024)
                .map(|value| Header {
                    name: name.to_string(),
                    value: value.to_string(),
                })
        })
        .collect()
}
fn into_response(response: PluginResponse) -> Response {
    into_response_parts(
        response.status,
        response.headers,
        Bytes::from(response.body),
    )
}

fn into_cached_response(response: CachedResponse) -> Response {
    into_response_parts(response.status, response.headers, response.body)
}

fn into_response_parts(status: u16, headers: Vec<Header>, body: Bytes) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut builder = Response::builder().status(status);
    for header in headers {
        let Ok(name) = HeaderName::try_from(header.name) else {
            continue;
        };
        if matches!(
            name.as_str(),
            "connection"
                | "content-length"
                | "keep-alive"
                | "proxy-authenticate"
                | "proxy-authorization"
                | "set-cookie"
                | "te"
                | "trailer"
                | "transfer-encoding"
                | "upgrade"
        ) {
            continue;
        }
        if let Ok(value) = HeaderValue::try_from(header.value) {
            builder = builder.header(name, value);
        }
    }
    builder
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}
fn error_response(error: Error) -> Response {
    match error {
        Error::Timeout => StatusCode::GATEWAY_TIMEOUT.into_response(),
        Error::Busy => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        Error::RateLimited => StatusCode::TOO_MANY_REQUESTS.into_response(),
        Error::PluginBadRequest(_) => StatusCode::BAD_REQUEST.into_response(),
        Error::PluginForbidden => StatusCode::FORBIDDEN.into_response(),
        Error::PluginNotFound => StatusCode::NOT_FOUND.into_response(),
        _ => StatusCode::BAD_GATEWAY.into_response(),
    }
}
fn method_filter(method: &str) -> Result<MethodFilter, Error> {
    match Method::from_bytes(method.as_bytes())
        .map_err(|_| Error::Manifest("invalid plugin HTTP method".into()))?
    {
        Method::GET => Ok(MethodFilter::GET),
        Method::POST => Ok(MethodFilter::POST),
        Method::PUT => Ok(MethodFilter::PUT),
        Method::PATCH => Ok(MethodFilter::PATCH),
        Method::DELETE => Ok(MethodFilter::DELETE),
        Method::HEAD => Ok(MethodFilter::HEAD),
        Method::OPTIONS => Ok(MethodFilter::OPTIONS),
        _ => Err(Error::Manifest("unsupported plugin HTTP method".into())),
    }
}
fn cache_capacity(count: usize) -> u64 {
    if count == 0 {
        return 0;
    }
    std::env::var("NUR_PLUGIN_CACHE_MEMORY_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| (1024 * 1024..=1024 * 1024 * 1024).contains(value))
        .unwrap_or(64 * 1024 * 1024)
        / count as u64
}
fn route_cache(policy: CachePolicy, capacity: u64) -> RouteCache {
    let minimum_weight = capacity
        .div_ceil(policy.max_entries.max(1))
        .min(u64::from(u32::MAX)) as u32;
    RouteCache {
        responses: Cache::builder()
            .max_capacity(capacity)
            .weigher(move |key: &String, cached: &CachedResponse| {
                cache_entry_weight(key, cached, minimum_weight)
            })
            .build(),
        ttl: policy.ttl,
        generation: Arc::new(AtomicU64::new(0)),
    }
}
impl RouteCache {
    fn key(&self, key: String) -> String {
        format!("{}:{key}", self.generation.load(Ordering::Acquire))
    }
    fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.responses.invalidate_all();
    }
}
fn cache_entry_weight(key: &str, cached: &CachedResponse, minimum_weight: u32) -> u32 {
    let header_bytes = cached.headers.iter().fold(0_u64, |total, header| {
        total.saturating_add((header.name.len() + header.value.len()) as u64)
    });
    u32::try_from(
        key.len() as u64
            + cached.body.len() as u64
            + header_bytes
            + std::mem::size_of::<CachedResponse>() as u64,
    )
    .unwrap_or(u32::MAX)
    .max(minimum_weight)
}
fn cache_key(route_id: &str, request: &PluginRequest) -> String {
    let mut key = format!("{}\n{}\n{}", route_id, request.method, request.path);
    if let Some(query) = &request.query {
        key.push('?');
        key.push_str(query);
    }
    for header in &request.headers {
        key.push('\n');
        key.push_str(&header.name);
        key.push(':');
        key.push_str(&header.value);
    }
    key
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc, time::Instant};

    use axum::{extract::Path, http::StatusCode};
    use bytes::Bytes;
    use nur_plugins::{CachePolicy, Header, Request as PluginRequest};

    use super::{
        CachedResponse, PluginCacheInvalidator, cache_entry_weight, cache_key, into_response_parts,
        is_plugin_write_method, plugin_path_params, request_identity, route_cache,
        valid_upload_chunk_range, validate_cached_request_body,
    };

    #[test]
    fn cached_response_body_clones_share_the_same_allocation() {
        let response = CachedResponse {
            status: 200,
            headers: Vec::new(),
            body: Bytes::from(vec![1, 2, 3]),
            expires_at: Instant::now(),
        };
        let clone = response.clone();

        assert_eq!(response.body.as_ptr(), clone.body.as_ptr());
    }

    #[test]
    fn public_routes_never_receive_an_authenticated_identity() {
        assert!(request_identity(false, 42, vec!["admin".into()]).is_none());
        let identity = request_identity(true, 42, vec!["admin".into()])
            .expect("protected route receives identity");
        assert_eq!(identity.user_id, 42);
    }

    #[test]
    fn resumable_upload_ranges_must_match_the_chunk_exactly() {
        assert!(valid_upload_chunk_range(0, 4, 8, 4));
        assert!(valid_upload_chunk_range(4, 8, 8, 4));
        assert!(!valid_upload_chunk_range(4, 4, 8, 0));
        assert!(!valid_upload_chunk_range(0, 5, 4, 5));
        assert!(!valid_upload_chunk_range(0, 4, 8, 3));
    }

    #[test]
    fn path_parameters_are_forwarded_in_stable_order() {
        let params = plugin_path_params(Some(Path(HashMap::from([
            ("year".into(), "2026".into()),
            ("slug".into(), "summer-festival".into()),
        ]))));
        assert_eq!(params[0], ("slug".into(), "summer-festival".into()));
        assert_eq!(params[1], ("year".into(), "2026".into()));
        assert!(plugin_path_params(None).is_empty());
    }

    #[test]
    fn cache_invalidation_changes_the_generation() {
        let cache = route_cache(
            CachePolicy {
                ttl: std::time::Duration::from_secs(60),
                max_entries: 4,
            },
            4096,
        );
        let before = cache.key("home".into());
        PluginCacheInvalidator {
            caches: Arc::new(vec![cache.clone()]),
        }
        .invalidate();
        assert_ne!(before, cache.key("home".into()));
    }

    #[test]
    fn cache_weight_accounts_for_payload_headers_and_entry_limit() {
        let response = CachedResponse {
            status: 200,
            headers: vec![Header {
                name: "content-type".into(),
                value: "text/plain".into(),
            }],
            body: Bytes::from(vec![0; 256]),
            expires_at: Instant::now(),
        };

        assert!(cache_entry_weight("cache-key", &response, 1) >= 256);
        assert_eq!(cache_entry_weight("a", &response, 512), 512);
    }

    #[test]
    fn cache_keys_include_method_query_and_every_forwarded_header() {
        let request = |method: &str, query: Option<&str>, user_agent: &str| PluginRequest {
            method: method.into(),
            path: "/events".into(),
            path_params: Vec::new(),
            query: query.map(Into::into),
            headers: vec![
                Header {
                    name: "accept".into(),
                    value: "text/html".into(),
                },
                Header {
                    name: "user-agent".into(),
                    value: user_agent.into(),
                },
            ],
            body: Vec::new(),
            client_ip: None,
            identity: None,
        };

        let desktop = cache_key("events", &request("GET", Some("year=2026"), "desktop"));
        let mobile = cache_key("events", &request("GET", Some("year=2026"), "mobile"));
        let head = cache_key("events", &request("HEAD", Some("year=2026"), "mobile"));
        let other_query = cache_key("events", &request("GET", Some("year=2027"), "desktop"));

        assert_ne!(desktop, mobile);
        assert_ne!(mobile, head);
        assert_ne!(desktop, other_query);
    }

    #[test]
    fn cached_routes_reject_bodies_and_writes_trigger_invalidation() {
        assert_eq!(
            validate_cached_request_body(true, b"content"),
            Err(StatusCode::BAD_REQUEST)
        );
        assert!(validate_cached_request_body(true, &[]).is_ok());
        assert!(validate_cached_request_body(false, b"content").is_ok());
        assert!(is_plugin_write_method("POST"));
        assert!(is_plugin_write_method("DELETE"));
        assert!(!is_plugin_write_method("GET"));
    }

    #[test]
    fn forbidden_plugin_response_headers_are_removed() {
        let response = into_response_parts(
            200,
            vec![
                Header {
                    name: "set-cookie".into(),
                    value: "session=secret".into(),
                },
                Header {
                    name: "content-type".into(),
                    value: "text/plain".into(),
                },
            ],
            Bytes::new(),
        );
        assert!(!response.headers().contains_key("set-cookie"));
        assert_eq!(response.headers()["content-type"], "text/plain");
    }
}
