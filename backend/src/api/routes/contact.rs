use axum::{
    Json,
    extract::State,
    extract::{OriginalUri, Path, Query},
};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;
use ts_rs::TS;

use crate::{
    db::{
        fields::{MailTargetFields, Table},
        handles,
        models::{AuthRole, Role},
        queries::{QueryObj, RespondObj},
    },
    mail::client::{Msg, message},
    utils::errors::ServiceError,
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
) -> Result<Json<RespondObj<AuthRole>>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        params.path = original_uri.path().into();
        params.query = original_uri.query().unwrap_or("").into();

        return match handles::select_record(&pool, &Table::AuthRoles, params).await {
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

pub async fn mailer(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(target): Path<String>,
    Json(contact): Json<Contact>,
) -> Result<(), ServiceError> {
    let target = handles::select_mail_target(&pool, &target).await?;
    let text = format!(
        r#"
        Name: {} | Mail: {}\n
        ---------------------------------------------------------\n\n
        {}
    "#,
        contact.name, contact.mail, contact.text
    );
    let msg = Msg::new(contact.mail, contact.name, None, text, target);

    message(msg).await?;

    Ok(())
}
