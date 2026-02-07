use std::convert::Infallible;

use async_stream::stream;
use axum::{
    Extension, Json,
    extract::{Query, State},
    response::sse::{Event, KeepAlive, Sse},
};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use real::RealIp;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Sender;
use tokio_stream::Stream;

use crate::{
    AuthUserMeta,
    db::models::Role,
    sse::{SseAuthState, UuidData, check_uuid, prune_uuids},
    utils::errors::NurError,
};

#[derive(Deserialize, Serialize)]
pub struct User {
    uuid: String,
}

impl User {
    fn new(uuid: String) -> Self {
        Self { uuid }
    }
}

pub async fn generate_uuid(
    real_ip: RealIp,
    Extension(user_meta): Extension<AuthUserMeta>,
    details: AuthDetails<Role>,
    State(data): State<SseAuthState>,
) -> Result<Json<User>, NurError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        let mut uuids = data.uuids.lock().await;
        let ip_address = real_ip.ip().to_string();
        let user_id = if user_meta.id > 0 {
            Some(user_meta.id)
        } else {
            None
        };
        let new_uuid = UuidData::new(ip_address, user_id);
        let user_auth = User::new(new_uuid.uuid.to_string());

        prune_uuids(&mut uuids);

        uuids.insert(new_uuid);

        return Ok(Json(user_auth));
    }

    Err(NurError::Forbidden(
        "You do not have permission to access this resource.".into(),
    ))
}

pub async fn sse_handler(
    real_ip: RealIp,
    State((tx, data)): State<(Sender<String>, SseAuthState)>,
    Query(user): Query<User>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, NurError> {
    let mut uuids = data.uuids.lock().await;
    let ip_address = real_ip.ip().to_string();
    check_uuid(&mut uuids, user.uuid.as_str(), &ip_address)?;

    let mut rx = tx.subscribe();

    Ok(Sse::new(stream! {
        while let Ok(msg) = rx.recv().await {
            yield Ok(Event::default().data::<String>(msg));
        }
    })
    .keep_alive(KeepAlive::default()))
}
