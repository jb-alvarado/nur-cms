use axum::{
    Json,
    extract::{OriginalUri, Query, State},
};
use protect_axum::authorities::{AuthDetails, AuthoritiesCheck};
use serde_json::Value;
use sqlx::postgres::PgPool;
use tokio::sync::broadcast::Sender;
use tracing::error;

use crate::{
    CMS_CONFIG, CONFIG,
    db::{
        fields::{ConfigurationFields, Table},
        handles,
        models::{BrandingConfiguration, CmsConfiguration, Configuration, Role},
        queries::QueryObj,
    },
    utils::editor_settings::{valid_entry_status, validate_hidden_entry_fields},
    utils::errors::NurError,
};

const MAX_CONFIGURATION_ITEMS: usize = 128;
const MAX_CONFIGURATION_ITEM_LENGTH: usize = 80;
const DISABLEABLE_FEATURES: &[&str] = &["comments"];

pub async fn branding_config_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
) -> Result<Json<BrandingConfiguration>, NurError> {
    Ok(Json(handles::select_branding_configuration(&pool).await?))
}

pub async fn cms_config_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
) -> Result<Json<CmsConfiguration>, NurError> {
    if !details
        .authorities
        .iter()
        .any(|role| !matches!(role, Role::Guest))
    {
        return Err(NurError::Forbidden(
            "You do not have permission to access this resource.".into(),
        ));
    }

    let configuration = handles::select_cms_configuration(&pool).await?;
    *CMS_CONFIG.write().await = configuration.clone();

    Ok(Json(configuration))
}

pub async fn cms_config_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(mut configuration): Json<CmsConfiguration>,
) -> Result<(), NurError> {
    if !details.has_any_authority(&[&Role::Admin]) {
        return Err(NurError::Forbidden(
            "You do not have permission to access this resource.".into(),
        ));
    }

    configuration.frontend_name = configuration.frontend_name.trim().to_string();
    validate_cms_configuration(&configuration)?;
    handles::update_cms_configuration(&pool, &configuration).await?;
    *CMS_CONFIG.write().await = configuration;

    Ok(())
}

pub async fn config_select(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    Query(mut params): Query<QueryObj<ConfigurationFields>>,
    details: AuthDetails<Role>,
    OriginalUri(original_uri): OriginalUri,
) -> Result<Json<Configuration>, NurError> {
    if !details.has_any_authority(&[&Role::Admin]) {
        return Err(NurError::Forbidden(
            "You do not have permission to access this resource.".into(),
        ));
    }

    params.path = original_uri.path().into();
    params.query = original_uri.query().unwrap_or("").into();

    match handles::select_record(&pool, &Table::Configuration, params).await {
        Ok(types) => Ok(Json(types.results.first().cloned().unwrap_or_default())),
        Err(e) => {
            error!("{e}");
            Err(NurError::InternalServerError)
        }
    }
}

pub async fn config_update(
    State((pool, _)): State<(PgPool, Sender<String>)>,
    details: AuthDetails<Role>,
    Json(content): Json<Value>,
) -> Result<(), NurError> {
    if details.has_any_authority(&[&Role::Admin]) {
        return match handles::update_record(&pool, &Table::Configuration, 1, &content).await {
            Ok(_) => {
                {
                    let config = handles::select_configuration(&pool).await?;
                    let mut cfg = CONFIG.write().await;
                    *cfg = config;
                }

                Ok(())
            }
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

fn validate_cms_configuration(configuration: &CmsConfiguration) -> Result<(), NurError> {
    if configuration.frontend_name.is_empty()
        || configuration.frontend_name.chars().count() > 160
        || configuration.frontend_name.chars().any(char::is_control)
    {
        return Err(NurError::InvalidInput);
    }

    if configuration
        .admin_language
        .as_deref()
        .is_some_and(|language| !valid_language_code(language))
    {
        return Err(NurError::InvalidInput);
    }

    if !valid_entry_status(&configuration.entry_default_status) {
        return Err(NurError::InvalidInput);
    }
    validate_hidden_entry_fields(&configuration.entry_hidden_fields)?;

    validate_string_list(&configuration.hidden_menu_items, |item| item != "comments")?;
    validate_string_list(&configuration.disabled_features, |item| {
        DISABLEABLE_FEATURES.contains(&item)
    })?;

    Ok(())
}

fn valid_language_code(language: &str) -> bool {
    if language.len() > 16 {
        return false;
    }

    let mut parts = language.split('-');
    let Some(primary) = parts.next() else {
        return false;
    };

    (2..=3).contains(&primary.len())
        && primary.bytes().all(|byte| byte.is_ascii_lowercase())
        && parts.all(|part| {
            (2..=8).contains(&part.len())
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

fn validate_string_list(items: &[String], allowed: impl Fn(&str) -> bool) -> Result<(), NurError> {
    if items.len() > MAX_CONFIGURATION_ITEMS {
        return Err(NurError::InvalidInput);
    }

    let mut unique = std::collections::HashSet::with_capacity(items.len());
    for item in items {
        let item = item.as_str();
        if item.is_empty()
            || item.len() > MAX_CONFIGURATION_ITEM_LENGTH
            || !item.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_:".contains(&byte)
            })
            || !allowed(item)
            || !unique.insert(item)
        {
            return Err(NurError::InvalidInput);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::db::models::CmsConfiguration;

    use super::validate_cms_configuration;

    #[test]
    fn validates_cms_configuration() {
        assert!(
            validate_cms_configuration(&CmsConfiguration {
                hidden_menu_items: vec!["authors".into(), "content:article".into()],
                disabled_features: vec!["comments".into()],
                ..Default::default()
            })
            .is_ok()
        );
    }

    #[test]
    fn rejects_unknown_features_and_duplicate_items() {
        assert!(
            validate_cms_configuration(&CmsConfiguration {
                disabled_features: vec!["media".into()],
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_cms_configuration(&CmsConfiguration {
                hidden_menu_items: vec!["authors".into(), "authors".into()],
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate_cms_configuration(&CmsConfiguration {
                hidden_menu_items: vec!["comments".into()],
                ..Default::default()
            })
            .is_err()
        );
    }

    #[test]
    fn rejects_invalid_frontend_names() {
        for frontend_name in [String::new(), "a".repeat(161), "bad\nname".into()] {
            assert!(
                validate_cms_configuration(&CmsConfiguration {
                    frontend_name,
                    ..Default::default()
                })
                .is_err()
            );
        }
    }

    #[test]
    fn validates_extensible_admin_language_codes() {
        assert!(
            validate_cms_configuration(&CmsConfiguration {
                admin_language: Some("fr".into()),
                ..Default::default()
            })
            .is_ok()
        );

        assert!(
            validate_cms_configuration(&CmsConfiguration {
                admin_language: Some("../de".into()),
                ..Default::default()
            })
            .is_err()
        );
    }

    #[test]
    fn cms_configuration_rejects_unknown_or_missing_fields() {
        assert!(
            serde_json::from_value::<CmsConfiguration>(json!({
                "frontend_name": "NUR CMS",
                "logo_media_id": null,
                "admin_language": null,
                "entry_default_status": "draft",
                "entry_hidden_fields": [],
                "hidden_menu_items": [],
                "disabled_features": [],
                "unknown": true
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<CmsConfiguration>(json!({
                "hidden_menu_items": []
            }))
            .is_err()
        );
    }
}
