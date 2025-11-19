use std::str::FromStr;

use axum::{
    Json,
    extract::{Extension, OriginalUri, Path, Query, State},
};
use chrono::Utc;
use markdown::{ParseOptions, to_html, to_mdast};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use serde_json::Value;
use sqlx::postgres::PgPool;
use strum::IntoEnumIterator;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::{
    CONFIG,
    db::{
        fields::{
            AuthRoleFields, AuthUserFields, ContentAuthorFields, ContentCategoryFields,
            ContentFields, ContentTypeFields, LocaleFields, MediaFields, OutputType, Table,
        },
        handles,
        models::{AuthRole, AuthUser, AuthUserMeta, ContentType, Locale, Role, TSConfig},
        queries::{QueryObj, RespondObj},
        serialize::*,
    },
    utils::{ast_serialize::to_structure_root, errors::ServiceError},
};

pub async fn auth_role_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<AuthRoleFields>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<AuthRole>>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        params.path = original_uri.path().to_string();
        params.query = original_uri.query().unwrap_or("").to_string();

        return match handles::select_record(&pool, &Table::AuthRoles, params).await {
            Ok(role) => Ok(Json(role)),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

pub async fn ts_language_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<TSConfig>>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::select_ts_language(&pool).await {
            Ok(lang) => Ok(Json(lang)),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

pub async fn auth_user_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<AuthUserFields>>,
    OriginalUri(original_uri): OriginalUri,
    Extension(user): Extension<AuthUserMeta>,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<AuthUserSerializer>>, ServiceError> {
    params.path = original_uri.path().to_string();

    if details.has_any_authority(&[&Role::Author, &Role::User]) {
        params.fields = vec![
            AuthUserFields::Email,
            AuthUserFields::FirstName,
            AuthUserFields::LastName,
            AuthUserFields::Username,
        ];
        params.query = format!("id={}", user.id);
    } else if details.has_any_authority(&[&Role::Admin]) {
        params.query = original_uri.query().unwrap_or("").to_string();
    } else {
        return Err(ServiceError::Forbidden(
            "You do not have permission to access this resource.".to_string(),
        ));
    };

    handles::select_auth_user(&pool, params)
        .await
        .map(Json)
        .map_err(|e| {
            error!("{e}");
            ServiceError::InternalServerError
        })
}

pub async fn auth_user_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::delete_record(&pool, &Table::AuthUsers, id).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

pub async fn auth_user_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(auth_user): Json<AuthUser>,
) -> Result<Json<i32>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::insert_record(&pool, &Table::AuthUsers, &auth_user).await {
            Ok(id) => Ok(Json(id)),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

pub async fn auth_user_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(auth_user): Json<AuthUser>,
) -> Result<(), ServiceError> {
    let mut auth_user: AuthUser = auth_user;
    auth_user.updated_at = Some(Utc::now());
    auth_user.last_login = None;

    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::update_record(&pool, &Table::AuthUsers, id, &auth_user).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

/* ------------------------------
LOCALES
---------------------------------*/

pub async fn locale_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<LocaleFields>>,
    OriginalUri(original_uri): OriginalUri,
) -> Result<Json<RespondObj<Locale>>, ServiceError> {
    params.path = original_uri.path().to_string();
    params.query = original_uri.query().unwrap_or("").to_string();

    return match handles::select_record(&pool, &Table::Locales, params).await {
        Ok(locale) => Ok(Json(locale)),
        Err(e) => {
            error!("{e}");
            Err(ServiceError::InternalServerError)
        }
    };
}

pub async fn locale_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::delete_record(&pool, &Table::Locales, id).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

pub async fn locale_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(locale): Json<Locale>,
) -> Result<Json<i32>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::insert_record(&pool, &Table::Locales, &locale).await {
            Ok(id) => Ok(Json(id)),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

pub async fn locale_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(locale): Json<Locale>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::update_record(&pool, &Table::Locales, id, &locale).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

/* ------------------------------
CONTENT AUTHOR
---------------------------------*/

pub async fn authors_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<ContentAuthorFields>>,
    OriginalUri(original_uri): OriginalUri,
) -> Result<Json<RespondObj<AuthorSerializer>>, ServiceError> {
    params.path = original_uri.path().to_string();
    params.query = original_uri.query().unwrap_or("").to_string();

    return match handles::select_content_author(&pool, params).await {
        Ok(authors) => Ok(Json(authors)),
        Err(e) => {
            error!("{e}");
            Err(ServiceError::InternalServerError)
        }
    };
}

pub async fn author_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<Json<i32>, ServiceError> {
    let table = Table::ContentAuthors;

    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::insert_record(&pool, &table, &content).await {
            Ok(id) => Ok(Json(id)),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

pub async fn author_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(mut content): Json<Value>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        content["updated_at"] = Value::String(Utc::now().to_rfc3339());

        return match handles::update_record(&pool, &Table::ContentAuthors, id, &content).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

pub async fn author_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::delete_record(&pool, &Table::ContentAuthors, id).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

/* ------------------------------
CONTENT CATEGORIES
---------------------------------*/

pub async fn categories_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<ContentCategoryFields>>,
    OriginalUri(original_uri): OriginalUri,
) -> Result<Json<RespondObj<ContentCategorySerializer>>, ServiceError> {
    params.path = original_uri.path().to_string();
    params.query = original_uri.query().unwrap_or("").to_string();

    match handles::select_categories(&pool, &params).await {
        Ok(types) => Ok(Json(types)),
        Err(e) => {
            error!("{e}");
            Err(ServiceError::InternalServerError)
        }
    }
}

pub async fn category_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<Json<i32>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::insert_record(&pool, &Table::ContentCategories, &content).await {
            Ok(id) => Ok(Json(id)),
            Err(e) => {
                error!("Insert {e}");
                let mut err = e.to_string();

                if err.contains("duplicate key") && err.contains("slug") {
                    err = "Duplicate slug, create a unique one!".to_string();
                }
                Err(ServiceError::Conflict(err))
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

pub async fn category_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::update_record(&pool, &Table::ContentCategories, id, &content).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

pub async fn category_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
        return match handles::delete_record(&pool, &Table::ContentCategories, id).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

/* ------------------------------
CONTENT
---------------------------------*/

pub async fn content_types_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<ContentTypeFields>>,
    OriginalUri(original_uri): OriginalUri,
) -> Result<Json<RespondObj<ContentType>>, ServiceError> {
    params.path = original_uri.path().to_string();
    params.query = original_uri.query().unwrap_or("").to_string();

    match handles::select_record(&pool, &Table::ContentTypes, params).await {
        Ok(types) => Ok(Json(types)),
        Err(e) => {
            error!("{e}");
            Err(ServiceError::InternalServerError)
        }
    }
}

pub async fn entries_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<ContentFields>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<RespondObj<ContentSerializer>>, ServiceError> {
    params.path = original_uri.path().to_string();
    params.query = original_uri.query().unwrap_or("").to_string();

    let mut output = CONFIG.read().await.output_type.clone();

    if let Some(typ) = &params.output_type
        && details.has_any_authority(&[&Role::Admin, &Role::Author])
    {
        output = typ.clone();
    }

    let mut content = handles::select_content(&pool, &params).await?;

    if params.fields.contains(&ContentFields::Body) && output != OutputType::Markdown {
        for b in &mut content.results {
            let text = b.text.take().unwrap_or_default();
            b.text = None;

            match output {
                OutputType::AST => {
                    let ast = to_mdast(&text, &ParseOptions::default())?;
                    let json = serde_json::to_string(&ast).unwrap_or_default();
                    let tree: Value = serde_json::from_str(&json).unwrap_or_default();
                    let body = to_structure_root(&tree, &mut b.media);

                    b.body = Some(body);
                }
                OutputType::HTML => {
                    let html = to_html(&text);
                    b.body = Some(Value::String(html));
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
    Query(mut params): Query<QueryObj<ContentFields>>,
    OriginalUri(original_uri): OriginalUri,
    details: AuthDetails<Role>,
) -> Result<Json<ContentSerializer>, ServiceError> {
    params.path = original_uri.path().to_string();
    params.query = original_uri.query().unwrap_or("").to_string();
    params.type_slug = Some(type_slug);
    params.search_slug = Some(slug);

    let mut output = CONFIG.read().await.output_type.clone();

    if let Some(typ) = &params.output_type
        && details.has_any_authority(&[&Role::Admin, &Role::Author])
    {
        output = typ.clone();
    }

    if let Some(mut content) = handles::select_content(&pool, &params)
        .await?
        .results
        .into_iter()
        .next()
    {
        if params.fields.contains(&ContentFields::Body) && output != OutputType::Markdown {
            let text = content.text.take().unwrap_or_default();

            match output {
                OutputType::AST => {
                    let ast = to_mdast(&text, &ParseOptions::default())?;
                    let tree: Value = serde_json::to_value(ast).unwrap_or_default();
                    let body = to_structure_root(&tree, &mut content.media);
                    content.body = Some(body);
                }
                OutputType::HTML => {
                    let html = to_html(&text);
                    content.body = Some(Value::String(html));
                }
                _ => {}
            }
        }

        return Ok(Json(content));
    }

    Err(ServiceError::NoContent)
}

pub async fn entry_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(id): Path<i32>,
    details: AuthDetails<Role>,
    Extension(user): Extension<AuthUserMeta>,
    Json(mut content): Json<Value>,
) -> Result<(), ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
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

        return match handles::update_record(&pool, &Table::ContentEntries, id, &content).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::Conflict(e.to_string()))
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
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
        "You do not have permission to access this resource.".to_string(),
    ))
}

pub async fn entry_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Extension(user): Extension<AuthUserMeta>,
    Json(mut content): Json<Value>,
) -> Result<Json<i32>, ServiceError> {
    if details.has_any_authority(&[&Role::Admin, &Role::Author]) {
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

        return match handles::insert_record(&pool, &Table::ContentEntries, &content).await {
            Ok(id) => Ok(Json(id)),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::Conflict(e.to_string()))
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

pub async fn content_delete(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path((table, id)): Path<(String, i32)>,
    details: AuthDetails<Role>,
) -> Result<(), ServiceError> {
    let table = Table::from_str(&format!("content_{table}"))?;

    if (table == Table::ContentTypes && details.has_any_authority(&[&Role::Admin]))
        || (table != Table::ContentTypes
            && Table::iter().any(|t| t == table)
            && details.has_any_authority(&[&Role::Admin, &Role::Author]))
    {
        return match handles::delete_record(&pool, &table, id).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

pub async fn content_insert(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path(table): Path<String>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<Json<i32>, ServiceError> {
    let table = Table::from_str(&format!("content_{table}"))?;

    if (table == Table::ContentTypes && details.has_any_authority(&[&Role::Admin]))
        || (table != Table::ContentTypes
            && Table::iter().any(|t| t == table)
            && details.has_any_authority(&[&Role::Admin, &Role::Author]))
    {
        return match handles::insert_record(&pool, &table, &content).await {
            Ok(id) => Ok(Json(id)),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

pub async fn content_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Path((table, id)): Path<(String, i32)>,
    details: AuthDetails<Role>,
    Json(mut content): Json<Value>,
) -> Result<(), ServiceError> {
    let table = Table::from_str(&format!("content_{table}"))?;

    if (table == Table::ContentTypes && details.has_any_authority(&[&Role::Admin]))
        || (table != Table::ContentTypes
            && Table::iter().any(|t| t == table)
            && details.has_any_authority(&[&Role::Admin, &Role::Author]))
    {
        content["updated_at"] = Value::String(Utc::now().to_rfc3339());

        return match handles::update_record(&pool, &table, id, &content).await {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        };
    }

    Err(ServiceError::Forbidden(
        "You do not have permission to access this resource.".to_string(),
    ))
}

/* ------------------------------
MEDIA
---------------------------------*/

pub async fn media_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<MediaFields>>,
    OriginalUri(original_uri): OriginalUri,
) -> Result<Json<RespondObj<MediaSerializer>>, ServiceError> {
    params.path = original_uri.path().to_string();
    params.query = original_uri.query().unwrap_or("").to_string();

    match handles::select_media(&pool, &params).await {
        Ok(media) => Ok(Json(media)),
        Err(e) => {
            error!("{e}");
            Err(ServiceError::InternalServerError)
        }
    }
}
