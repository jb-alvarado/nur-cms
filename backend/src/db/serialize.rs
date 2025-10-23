use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, Row, postgres::PgRow};
use ts_rs::TS;

use crate::db::{
    fields::ColumnCounter,
    is_zero,
    models::{AuthRole, Role},
};

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct AuthUserSerializer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(default, skip_serializing)]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<AuthRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_login: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for AuthUserSerializer {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let mut role = None;

        if let Ok((id, name)) = row.try_get::<(i32, String), &str>("auth_role") {
            role = Some(AuthRole {
                id,
                name: Role::set_role(&name),
                total_count: None,
            });
        };

        Ok(Self {
            id: row.try_get("id").ok(),
            email: row.try_get("email").ok(),
            username: row.try_get("username").ok(),
            first_name: row.try_get("first_name").ok(),
            last_name: row.try_get("last_name").ok(),
            password: row.try_get("password").ok(),
            role_id: row.try_get("role_id").ok(),
            role,
            created_at: None,
            updated_at: None,
            last_login: row.try_get("last_login").ok(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for AuthUserSerializer {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct ContentSerializer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>, // draft, published, archived
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<AuthUserSerializer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<ContentCategorySerializer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<ContentTagSerializer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<ContentAttributeSerializer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<ContentBlockSerializer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<MediaSerializer>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentSerializer {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let mut author = None;
        let mut categories = vec![];
        let mut tags = vec![];
        let mut attributes = vec![];
        if let Ok((id, first_name, last_name)) =
            row.try_get::<(i32, String, String), &str>("author")
        {
            author = Some(AuthUserSerializer {
                id: Some(id),
                first_name: Some(first_name),
                last_name: Some(last_name),
                ..Default::default()
            });
        };

        for (id, name, slug) in row
            .try_get::<Vec<Option<(i32, String, String)>>, &str>("categories")
            .unwrap_or_default()
            .into_iter()
            .flatten()
        {
            categories.push(ContentCategorySerializer { id, name, slug });
        }

        for (id, name, slug) in row
            .try_get::<Vec<Option<(i32, String, String)>>, &str>("tags")
            .unwrap_or_default()
            .into_iter()
            .flatten()
        {
            tags.push(ContentTagSerializer { id, name, slug });
        }

        for (id, name, value) in row
            .try_get::<Vec<Option<(i32, String, Value)>>, &str>("attributes")
            .unwrap_or_default()
            .into_iter()
            .flatten()
        {
            attributes.push(ContentAttributeSerializer { id, name, value });
        }

        let blocks = row
            .try_get::<Option<serde_json::Value>, _>("blocks")
            .unwrap_or_default()
            .map(|v| serde_json::from_value::<Vec<ContentBlockSerializer>>(v).unwrap_or_default())
            .unwrap_or_default();

        let media = row
            .try_get::<Option<serde_json::Value>, _>("media")
            .unwrap_or_default()
            .map(|v| serde_json::from_value::<Vec<MediaSerializer>>(v).unwrap_or_default())
            .unwrap_or_default();

        Ok(Self {
            id: row.try_get("id").ok(),
            slug: row.try_get("slug").ok(),
            status: row.try_get("status").ok(),
            author,
            categories,
            tags,
            attributes,
            blocks,
            locale: row.try_get("locale").ok(),
            title: row.try_get("title").ok(),
            description: row.try_get("description").ok(),
            text: row.try_get("text").ok(),
            body: None,
            created_at: row.try_get("created_at").ok(),
            updated_at: row.try_get("updated_at").ok(),
            media,
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentSerializer {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct ContentCategorySerializer {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slug: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct ContentTagSerializer {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slug: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct ContentAttributeSerializer {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default)]
    pub value: Value,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct ContentBlockSerializer {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub r#type: String,
    #[serde(default)]
    pub data: Value,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct MediaSerializer {
    pub id: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    pub filename: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub ast_line: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<MediaVariantSerializer>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct MediaVariantSerializer {
    pub resolution: i32,
    pub format: String,
    pub filename: String,
}
