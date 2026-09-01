use axum::{
    Json,
    extract::State,
    extract::{OriginalUri, Path, Query},
};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use real::RealIp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;
use ts_rs::TS;

use crate::{
    api::request::ApiJson,
    db::{
        fields::{MailTargetFields, Table},
        handles,
        models::{MailTarget, Role},
        queries::{QueryObj, RespondObj},
    },
    mail::service::{MailRequest, deliver_mail, prepare_mail_with_evaluation},
    utils::{errors::NurError, spam_detection::evaluate_text},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
pub struct Contact {
    pub email: String,
    pub subject: Option<String>,
    pub name: String,
    pub text: String,
}

pub async fn targets_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<MailTargetFields>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<MailTarget>>, NurError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        params.path = original_uri.path().into();
        params.query = original_uri.query().unwrap_or("").into();

        return match handles::select_record(&pool, &Table::MailTargets, params).await {
            Ok(role) => Ok(Json(role)),
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

pub async fn target_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<Json<i32>, NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::insert_record(&pool, &Table::MailTargets, &content).await {
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

pub async fn target_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::update_record(&pool, &Table::MailTargets, id, &content).await {
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

pub async fn target_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::delete_record(&pool, &Table::MailTargets, id).await {
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

pub async fn mailer(
    real_ip: RealIp,
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(target): Path<String>,
    ApiJson(contact): ApiJson<Contact>,
) -> Result<(), NurError> {
    let evaluation = evaluate_text(&contact.text, None);
    let spam_score = evaluation.score;
    let prepared = match prepare_mail_with_evaluation(
        MailRequest {
            reply_to: contact.email,
            subject: contact.subject,
            name: contact.name,
            text: contact.text,
        },
        evaluation,
    )
    .await
    {
        Ok(prepared) => prepared,
        Err(error) => {
            if matches!(error, NurError::Conflict(_)) {
                // Keep the client identity in the HTTP endpoint's log without exposing it to plugins.
                error!(
                    "Spam detected from: {:?}, score: {:?}",
                    real_ip.ip(),
                    spam_score
                );
            }
            return Err(error);
        }
    };
    let mut contact_mail = prepared;
    contact_mail.text = format!(
        "Name: {}\nMail: {}\n------------------------------------\n\n{}",
        contact_mail.name, contact_mail.reply_to, contact_mail.text
    );
    deliver_mail(&pool, &target, contact_mail).await
}
