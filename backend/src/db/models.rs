use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, postgres::PgRow};
use ts_rs::TS;

use crate::db::{
    fields::{ColumnCounter, OutputType},
    is_zero,
};

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Author,
    User,
    #[default]
    Guest,
}

impl Role {
    pub fn set_role(role: &str) -> Self {
        role.parse().unwrap_or(Self::Guest)
    }
}

impl FromStr for Role {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "admin" => Ok(Self::Admin),
            "author" => Ok(Self::Author),
            "user" => Ok(Self::User),
            _ => Ok(Self::Guest),
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Admin => write!(f, "admin"),
            Self::Author => write!(f, "author"),
            Self::User => write!(f, "user"),
            Self::Guest => write!(f, "guest"),
        }
    }
}

#[derive(Clone, Debug, Default, FromRow, Deserialize, Serialize, TS)]
pub struct Configuration {
    pub id: i32,
    pub jwt_secret: String,
    pub output_type: OutputType,
    pub storage: Option<String>,
    pub mail_smtp: Option<String>,
    pub mail_user: Option<String>,
    pub mail_password: Option<String>,
    pub mail_starttls: bool,
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize, FromRow, TS)]
#[ts(export, export_to = "models.d.ts")]
#[serde(rename_all = "snake_case")]
pub struct AuthRole {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    pub name: Role,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for AuthRole {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let role = match row.get("name") {
            "admin" => Role::Admin,
            "author" => Role::Author,
            "user" => Role::User,
            _ => Role::Guest,
        };

        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            name: role,
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for AuthRole {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
pub struct AuthUser {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub role_id: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_login: Option<DateTime<Utc>>,
}

impl AuthUser {
    pub fn new(
        email: String,
        username: String,
        first_name: String,
        last_name: String,
        password: String,
        role_id: i32,
    ) -> Self {
        Self {
            email,
            username,
            first_name,
            last_name,
            password,
            role_id,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, Hash, PartialEq)]
pub struct AuthUserMeta {
    pub id: i32,
}

impl AuthUserMeta {
    pub fn new(id: i32) -> Self {
        Self { id }
    }
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "snake_case")]
pub struct TSConfig {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cfgname: String,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl ColumnCounter for TSConfig {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
pub struct Locale {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tsv_dict: String,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for Locale {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            code: row.try_get("code").unwrap_or_default(),
            name: row.try_get("name").unwrap_or_default(),
            tsv_dict: row.try_get("tsv_dict").unwrap_or_default(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for Locale {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
pub struct ContentType {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slug: String,
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentType {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            name: row.try_get("name").unwrap_or_default(),
            slug: row.try_get("slug").unwrap_or_default(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentType {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
pub struct ContentCategory {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub locale_id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slug: String,
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentCategory {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            locale_id: row.try_get("locale_id").unwrap_or_default(),
            name: row.try_get("name").unwrap_or_default(),
            slug: row.try_get("slug").unwrap_or_default(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentCategory {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
pub struct ContentTag {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub locale_id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slug: String,
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentTag {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            locale_id: row.try_get("locale_id").unwrap_or_default(),
            name: row.try_get("name").unwrap_or_default(),
            slug: row.try_get("slug").unwrap_or_default(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentTag {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
pub struct ContentEntry {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<i64>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub locale_id: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub type_id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slug: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub created_by: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub updated_by: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentEntry {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            group_id: row.try_get("group_id").ok(),
            locale_id: row.try_get("locale_id").unwrap_or_default(),
            type_id: row.try_get("type_id").unwrap_or_default(),
            slug: row.try_get("slug").unwrap_or_default(),
            title: row.try_get("title").unwrap_or_default(),
            description: row.try_get("description").unwrap_or_default(),
            text: row.try_get("text").unwrap_or_default(),
            status: row.try_get("status").unwrap_or_default(),
            created_by: row.try_get("created_by").unwrap_or_default(),
            updated_by: row.try_get("updated_by").unwrap_or_default(),
            created_at: row.try_get("created_at").ok(),
            updated_at: row.try_get("updated_at").ok(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentEntry {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
pub struct ContentAuthor {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentAuthor {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            first_name: row.try_get("first_name").unwrap_or_default(),
            last_name: row.try_get("last_name").unwrap_or_default(),
            slug: row.try_get("slug").unwrap_or_default(),
            bio: row.try_get("bio").ok(),
            photo: row.try_get("photo").ok(),
            created_at: row.try_get("created_at").ok(),
            updated_at: row.try_get("updated_at").ok(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentAuthor {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
pub struct ContentMeta {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub entry_id: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentMeta {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            entry_id: row.try_get("entry_id").unwrap_or_default(),
            data: row.try_get("data").unwrap_or_default(),
            start_time: row.try_get("start_time").ok(),
            end_time: row.try_get("end_time").ok(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentMeta {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
pub struct Media {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub filename: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uploaded_by: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for Media {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            alt: row.try_get("alt").unwrap_or_default(),
            filename: row.try_get("filename").unwrap_or_default(),
            path: row.try_get("path").unwrap_or_default(),
            r#type: row.try_get("type").ok(),
            uploaded_by: row.try_get("uploaded_by").ok(),
            created_at: row.try_get("created_at").ok(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for Media {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
pub struct MediaVariant {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub media_id: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub resolution: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub format: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub filename: String,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for MediaVariant {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            media_id: row.try_get("media_id").unwrap_or_default(),
            resolution: row.try_get("resolution").unwrap_or_default(),
            format: row.try_get("format").unwrap_or_default(),
            filename: row.try_get("filename").unwrap_or_default(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for MediaVariant {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}
