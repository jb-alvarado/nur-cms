use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, postgres::PgRow};

use crate::db::{fields::ColumnCounter, is_null, is_zero};

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize, FromRow)]
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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Locale {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for Locale {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            code: row.try_get("code").unwrap_or_default(),
            name: row.try_get("name").unwrap_or_default(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for Locale {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ContentType {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentType {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            name: row.try_get("name").unwrap_or_default(),
            description: row.try_get("description").ok(),
            created_at: row.try_get("created_at").ok(),
            updated_at: row.try_get("updated_at").ok(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentType {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ContentField {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub content_type_id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub field_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_index: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentField {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            content_type_id: row.try_get("content_type_id").unwrap_or_default(),
            name: row.try_get("name").unwrap_or_default(),
            field_type: row.try_get("field_type").unwrap_or_default(),
            required: row.try_get("required").ok(),
            order_index: row.try_get("order_index").ok(),
            created_at: row.try_get("created_at").ok(),
            updated_at: row.try_get("updated_at").ok(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentField {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ContentItem {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub content_type_id: i32,
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

impl FromRow<'_, PgRow> for ContentItem {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            content_type_id: row.try_get("content_type_id").unwrap_or_default(),
            status: row.try_get("status").unwrap_or_default(),
            created_by: row.try_get("created_by").unwrap_or_default(),
            updated_by: row.try_get("updated_by").unwrap_or_default(),
            created_at: row.try_get("created_at").ok(),
            updated_at: row.try_get("updated_at").ok(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentItem {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ContentValue {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub content_item_id: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub locale_id: i32,
    #[serde(default, skip_serializing_if = "is_null")]
    pub value: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentValue {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            content_item_id: row.try_get("content_item_id").unwrap_or_default(),
            locale_id: row.try_get("locale_id").unwrap_or_default(),
            value: row.try_get("value").unwrap_or_default(),
            created_at: row.try_get("created_at").ok(),
            updated_at: row.try_get("updated_at").ok(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for ContentValue {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
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
