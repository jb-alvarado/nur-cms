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
pub struct AuthorSerializer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaSerializer>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for AuthorSerializer {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let media = row
            .try_get::<Option<serde_json::Value>, _>("media")
            .unwrap_or_default()
            .map(|v| serde_json::from_value::<MediaSerializer>(v).unwrap_or_default());

        Ok(Self {
            id: row.try_get("id").ok(),
            first_name: row.try_get("first_name").ok(),
            last_name: row.try_get("last_name").ok(),
            slug: row.try_get("slug").ok(),
            bio: row.try_get("bio").ok(),
            media_id: row.try_get("media_id").ok(),
            media,
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for AuthorSerializer {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct ContentSerializer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    #[ts(as = "Option<i32>")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>, // draft, published, archived
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<AuthorSerializer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<ContentMetaSerializer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<ContentCategorySerializer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<ContentTagSerializer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<ContentBlockSerializer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "any")]
    pub body: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaSerializer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embeds: Vec<MediaSerializer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_members: Vec<GroupMemberSerializer>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentSerializer {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let mut tags = vec![];

        let author = row
            .try_get::<(Option<i32>, Option<String>, Option<String>, Option<i32>), &str>("author")
            .ok()
            .map(|(id, first_name, last_name, media_id)| AuthorSerializer {
                id,
                first_name,
                last_name,
                media_id,
                ..Default::default()
            });

        let meta = row
            .try_get::<(Option<Value>, Option<DateTime<Utc>>, Option<DateTime<Utc>>), &str>("meta")
            .ok()
            .filter(|(data, start_time, end_time)| {
                data.is_some() || start_time.is_some() || end_time.is_some()
            })
            .map(|(data, start_time, end_time)| ContentMetaSerializer {
                data,
                start_time,
                end_time,
            });

        let category = row
            .try_get::<Option<serde_json::Value>, _>("category")
            .unwrap_or_default()
            .map(|v| serde_json::from_value::<ContentCategorySerializer>(v).unwrap_or_default());

        for (id, name, slug) in row
            .try_get::<Vec<Option<(i32, String, String)>>, &str>("tags")
            .unwrap_or_default()
            .into_iter()
            .flatten()
        {
            tags.push(ContentTagSerializer {
                id: Some(id),
                name: Some(name),
                slug: Some(slug),
                total_count: None,
            });
        }

        let blocks = row
            .try_get::<Option<serde_json::Value>, _>("blocks")
            .unwrap_or_default()
            .map(|v| serde_json::from_value::<Vec<ContentBlockSerializer>>(v).unwrap_or_default())
            .unwrap_or_default();

        let group_members = row
            .try_get::<Option<serde_json::Value>, _>("group_members")
            .unwrap_or_default()
            .map(|v| serde_json::from_value::<Vec<GroupMemberSerializer>>(v).unwrap_or_default())
            .unwrap_or_default();

        let media = row
            .try_get::<Option<serde_json::Value>, _>("media")
            .unwrap_or_default()
            .map(|v| serde_json::from_value::<MediaSerializer>(v).unwrap_or_default());

        let embeds = row
            .try_get::<Option<serde_json::Value>, _>("embeds")
            .unwrap_or_default()
            .map(|v| serde_json::from_value::<Vec<MediaSerializer>>(v).unwrap_or_default())
            .unwrap_or_default();

        Ok(Self {
            id: row.try_get("id").ok(),
            group_id: row.try_get("group_id").ok(),
            category_id: row.try_get("category_id").ok(),
            locale_id: row.try_get("locale_id").ok(),
            media_id: row.try_get("media_id").ok(),
            slug: row.try_get("slug").ok(),
            status: row.try_get("status").ok(),
            author,
            meta,
            category,
            tags,
            blocks,
            title: row.try_get("title").ok(),
            description: row.try_get("description").ok(),
            text: row.try_get("text").ok(),
            body: None,
            created_at: row.try_get("created_at").ok(),
            updated_at: row.try_get("updated_at").ok(),
            group_members,
            media,
            embeds,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    #[ts(as = "Option<i32>")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaSerializer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_members: Vec<GroupMemberSerializer>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentCategorySerializer {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let media = row
            .try_get::<Option<serde_json::Value>, _>("media")
            .unwrap_or_default()
            .map(|v| serde_json::from_value::<MediaSerializer>(v).unwrap_or_default());

        let group_members = row
            .try_get::<Option<serde_json::Value>, _>("group_members")
            .unwrap_or_default()
            .and_then(|v| serde_json::from_value::<Vec<GroupMemberSerializer>>(v).ok())
            .unwrap_or_default();

        Ok(Self {
            id: row.try_get("id").ok(),
            group_id: row.try_get("group_id").ok(),
            locale_id: row.try_get("locale_id").ok(),
            name: row.try_get("name").ok(),
            slug: row.try_get("slug").ok(),
            status: row.try_get("status").ok(),
            media_id: row.try_get("media_id").ok(),
            media,
            group_members,
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentCategorySerializer {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct ContentTagSerializer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentTagSerializer {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").ok(),
            name: row.try_get("name").ok(),
            slug: row.try_get("slug").ok(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentTagSerializer {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct ContentMetaSerializer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_time: Option<DateTime<Utc>>,
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
pub struct GroupMemberSerializer {
    pub id: i32,
    pub locale_id: i32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct MediaSerializer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[ts(as = "i32")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ast_line: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<MediaVariantSerializer>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for MediaSerializer {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let variants = row
            .try_get::<Option<serde_json::Value>, _>("variants")
            .unwrap_or_default()
            .map(|v| serde_json::from_value::<Vec<MediaVariantSerializer>>(v).unwrap_or_default())
            .unwrap_or_default();

        Ok(Self {
            id: row.try_get("id").ok(),
            alt: row.try_get("alt").ok(),
            filename: row.try_get("filename").ok(),
            path: row.try_get("path").ok(),
            r#type: row.try_get("type").ok(),
            width: row.try_get("width").ok(),
            height: row.try_get("height").ok(),
            size: row.try_get("size").ok(),
            ast_line: row.try_get("ast_line").ok(),
            start_offset: row.try_get("start_offset").ok(),
            end_offset: row.try_get("end_offset").ok(),
            created_at: row.try_get("created_at").ok(),
            variants,
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for MediaSerializer {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "serialized.d.ts")]
pub struct MediaVariantSerializer {
    #[ts(as = "i32")]
    pub id: i64,
    pub width: i32,
    pub height: i32,
    pub filename: String,
}
