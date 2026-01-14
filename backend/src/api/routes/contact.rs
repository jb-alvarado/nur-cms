use axum::{
    Json,
    extract::State,
    extract::{OriginalUri, Path, Query},
};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;
use ts_rs::TS;

use crate::{
    db::{
        fields::{MailTargetFields, Table},
        handles,
        models::{MailTarget, Role},
        queries::{QueryObj, RespondObj},
    },
    mail::client::{Msg, message},
    utils::{errors::ServiceError, spam_detection::evaluate_text},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
pub struct Contact {
    pub mail: String,
    pub name: String,
    pub text: String,
}

pub async fn targets_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<MailTargetFields>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<MailTarget>>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        params.path = original_uri.path().into();
        params.query = original_uri.query().unwrap_or("").into();

        return match handles::select_record(&pool, &Table::MailTargets, params).await {
            Ok(role) => Ok(Json(role)),
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

pub async fn target_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<Json<i32>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::insert_record(&pool, &Table::MailTargets, &content).await {
            Ok(id) => Ok(Json(id)),
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

pub async fn target_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::update_record(&pool, &Table::MailTargets, id, &content).await {
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

pub async fn target_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::delete_record(&pool, &Table::MailTargets, id).await {
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

pub async fn mailer(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(target): Path<String>,
    Json(contact): Json<Contact>,
) -> Result<(), ServiceError> {
    let result = evaluate_text(&contact.text, None);

    if !result.passed {
        return Err(ServiceError::Conflict(
            "This message is not allowed!".to_string(),
        ));
    }

    let target = handles::select_mail_target(&pool, &target).await?;
    let text = format!(
        "Name: {}\nMail: {}\n------------------------------------\n\n{}",
        contact.name, contact.mail, contact.text
    );
    let msg = Msg::new(contact.mail, contact.name, None, text, target);

    message(msg).await?;

    Ok(())
}
