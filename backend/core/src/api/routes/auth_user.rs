use axum::{
    Json,
    extract::{Extension, OriginalUri, Path, Query, State},
};
use chrono::Utc;
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::{
    db::{
        fields::{AuthRoleFields, AuthUserFields, Table},
        handles,
        models::{AuthRole, AuthUser, AuthUserMeta, Role},
        queries::{QueryObj, RespondObj},
        serialize::*,
    },
    utils::errors::NurError,
};

pub async fn auth_role_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<AuthRoleFields>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<AuthRole>>, NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        params.path = original_uri.path().into();
        params.query = original_uri.query().unwrap_or("").into();

        return match handles::select_record(&pool, &Table::AuthRoles, params).await {
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

pub async fn auth_user_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<AuthUserFields>>,
    OriginalUri(original_uri): OriginalUri,
    Extension(user): Extension<AuthUserMeta>,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<AuthUserSerializer>>, NurError> {
    params.path = original_uri.path().into();

    if details.has_any_authority(&[&Role::Admin]) {
        params.query = original_uri.query().unwrap_or("").into();
    } else if details.has_any_authority(&[&Role::Author, &Role::User])
        || details
            .authorities
            .iter()
            .any(|role| matches!(role, Role::Custom(_)))
    {
        params.fields = vec![
            AuthUserFields::Email,
            AuthUserFields::FirstName,
            AuthUserFields::LastName,
            AuthUserFields::Username,
        ];
        params.query = format!("id={}", user.id);
    } else {
        return Err(NurError::Forbidden(
            "You do not have permission to access this resource.".into(),
        ));
    };

    handles::select_auth_user(&pool, params)
        .await
        .map(Json)
        .map_err(|e| {
            error!("{e}");
            NurError::InternalServerError
        })
}

pub async fn auth_user_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::delete_record(&pool, &Table::AuthUsers, id).await {
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

pub async fn auth_user_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(auth_user): Json<AuthUser>,
) -> Result<Json<i32>, NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::insert_record(&pool, &Table::AuthUsers, &auth_user).await {
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

pub async fn auth_user_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(auth_user): Json<AuthUser>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        let mut auth_user: AuthUser = auth_user;
        auth_user.updated_at = Some(Utc::now());
        auth_user.last_login = None;

        return match handles::update_record(&pool, &Table::AuthUsers, id, &auth_user).await {
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
