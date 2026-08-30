use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, Row, postgres::PgRow};
use ts_rs::TS;

use crate::db::{
    fields::{ColumnCounter, OutputType},
    is_zero,
};

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case", from = "String", into = "String")]
pub enum Role {
    Admin,
    Author,
    User,
    #[default]
    Guest,
    Custom(String),
}

impl From<Role> for String {
    fn from(value: Role) -> Self {
        value.to_string()
    }
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
            "guest" => Ok(Self::Guest),
            custom => Ok(Self::Custom(custom.to_string())),
        }
    }
}

impl From<String> for Role {
    fn from(value: String) -> Self {
        value.parse().unwrap_or(Self::Guest)
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Admin => write!(f, "admin"),
            Self::Author => write!(f, "author"),
            Self::User => write!(f, "user"),
            Self::Guest => write!(f, "guest"),
            Self::Custom(ref role) => write!(f, "{role}"),
        }
    }
}

impl PartialEq<Role> for str {
    fn eq(&self, other: &Role) -> bool {
        match other {
            Role::Admin => self.eq("admin"),
            Role::Author => self.eq("author"),
            Role::User => self.eq("user"),
            Role::Guest => self.eq("guest"),
            Role::Custom(role) => self.eq(role.as_str()),
        }
    }
}

impl PartialEq<Role> for String {
    fn eq(&self, other: &Role) -> bool {
        self.as_str() == other
    }
}

#[derive(Clone, Debug, Default, FromRow, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
#[serde(rename_all = "snake_case")]
pub struct Configuration {
    pub id: i32,
    #[serde(default, skip_serializing)]
    pub jwt_secret: String,
    pub output_type: OutputType,
    pub mail_smtp: Option<String>,
    pub mail_port: Option<i32>,
    pub mail_user: Option<String>,
    #[serde(default, skip_serializing)]
    pub mail_password: Option<String>,
    pub mail_starttls: bool,
    pub notification_emails: Option<Vec<String>>,
    pub image_extensions: Option<Vec<String>>,
    pub image_resolutions: Option<Vec<i32>>,
}

impl ColumnCounter for Configuration {
    fn total_count(&self) -> i64 {
        1
    }
}

#[derive(Clone, Debug, FromRow, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CmsConfiguration {
    pub frontend_name: String,
    pub logo_media_id: Option<i32>,
    pub admin_language: Option<String>,
    pub entry_default_status: String,
    pub entry_hidden_fields: Vec<String>,
    pub hidden_menu_items: Vec<String>,
    pub disabled_features: Vec<String>,
}

impl Default for CmsConfiguration {
    fn default() -> Self {
        Self {
            frontend_name: "NUR CMS".into(),
            logo_media_id: None,
            admin_language: None,
            entry_default_status: "draft".into(),
            entry_hidden_fields: Vec::new(),
            hidden_menu_items: Vec::new(),
            disabled_features: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, FromRow, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
#[serde(rename_all = "snake_case")]
pub struct BrandingConfiguration {
    pub frontend_name: String,
    pub logo_url: Option<String>,
    pub logo_alt: Option<String>,
    pub admin_language: Option<String>,
}

impl Default for BrandingConfiguration {
    fn default() -> Self {
        Self {
            frontend_name: "NUR CMS".into(),
            logo_url: None,
            logo_alt: None,
            admin_language: None,
        }
    }
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize, TS)]
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
        let role_name: String = row.try_get("name").unwrap_or_default();
        let role: Role = role_name.into();

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
    #[ts(skip)]
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
    #[serde(default)]
    pub order_index: i32,
    #[serde(default)]
    pub use_meta: bool,
    #[serde(default)]
    pub entry_default_status: Option<String>,
    #[serde(default)]
    pub entry_hidden_fields: Vec<String>,
    #[ts(skip)]
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentType {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            name: row.try_get("name").unwrap_or_default(),
            slug: row.try_get("slug").unwrap_or_default(),
            order_index: row.try_get("order_index").unwrap_or_default(),
            use_meta: row.try_get("use_meta").unwrap_or(false),
            entry_default_status: row.try_get("entry_default_status").ok(),
            entry_hidden_fields: row.try_get("entry_hidden_fields").unwrap_or_default(),
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
    #[ts(as = "i32")]
    #[serde(default, skip_serializing_if = "is_zero")]
    pub group_id: i64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub locale_id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slug: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_id: Option<i32>,
    #[ts(skip)]
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentCategory {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            group_id: row.try_get("group_id").unwrap_or_default(),
            locale_id: row.try_get("locale_id").unwrap_or_default(),
            name: row.try_get("name").unwrap_or_default(),
            slug: row.try_get("slug").unwrap_or_default(),
            status: row.try_get("status").unwrap_or_default(),
            media_id: row.try_get("media_id").ok(),
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slug: String,
    #[ts(skip)]
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentTag {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
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
    #[ts(as = "Option<i32>")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<i64>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub type_id: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_id: Option<i32>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub locale_id: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_id: Option<i32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slug: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
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
    #[ts(skip)]
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for ContentEntry {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            group_id: row.try_get("group_id").ok(),
            type_id: row.try_get("type_id").unwrap_or_default(),
            category_id: row.try_get("category_id").ok(),
            locale_id: row.try_get("locale_id").unwrap_or_default(),
            media_id: row.try_get("media_id").ok(),
            slug: row.try_get("slug").unwrap_or_default(),
            title: row.try_get("title").unwrap_or_default(),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[ts(skip)]
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
            media_id: row.try_get("media_id").ok(),
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
    #[ts(skip)]
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
    #[serde(default, skip_serializing_if = "is_zero")]
    pub width: Option<i32>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub height: Option<i32>,
    #[ts(as = "Option<i32>")]
    #[serde(default, skip_serializing_if = "is_zero")]
    pub size: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uploaded_by: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[ts(skip)]
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
            width: row.try_get("width").ok(),
            height: row.try_get("height").ok(),
            size: row.try_get("size").ok(),
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
    #[ts(as = "i32")]
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: i64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub media_id: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub width: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub height: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub filename: String,
    #[ts(skip)]
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for MediaVariant {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            media_id: row.try_get("media_id").unwrap_or_default(),
            width: row.try_get("width").unwrap_or_default(),
            height: row.try_get("height").unwrap_or_default(),
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

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
pub struct ContentNodeMedia {
    #[ts(as = "i32")]
    pub node_id: i64,
    pub media_id: i32,
    pub ast_line: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<i32>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, FromRow, TS)]
#[ts(export, export_to = "models.d.ts")]
#[serde(rename_all = "snake_case")]
pub struct ContentNodeTemplate {
    #[serde(default)]
    pub id: i32,
    pub name: String,
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    #[sqlx(json)]
    pub schema: Vec<ContentNodeDataField>,
    #[ts(skip)]
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "models.d.ts")]
#[serde(rename_all = "snake_case")]
pub enum ContentNodeDataKind {
    #[default]
    String,
    Text,
    Boolean,
    Number,
    Json,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
#[serde(rename_all = "snake_case")]
pub struct ContentNodeDataField {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub kind: ContentNodeDataKind,
    #[serde(default)]
    pub default: Value,
}

impl ContentNodeDataKind {
    fn matches_value(&self, value: &Value) -> bool {
        match self {
            Self::String | Self::Text => value.is_string(),
            Self::Boolean => value.is_boolean(),
            Self::Number => value.is_number(),
            Self::Json => true,
        }
    }
}

const MAX_NODE_TEMPLATE_DATA_BYTES: usize = 262_144;

impl ContentNodeTemplate {
    pub fn validate_schema(&self) -> Result<(), String> {
        const MAX_TEMPLATE_NAME_LENGTH: usize = 255;
        const MAX_TEMPLATE_FIELDS: usize = 64;

        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > MAX_TEMPLATE_NAME_LENGTH {
            return Err(format!(
                "Node template name must contain between 1 and {MAX_TEMPLATE_NAME_LENGTH} characters"
            ));
        }
        if self.schema.len() > MAX_TEMPLATE_FIELDS {
            return Err(format!(
                "A node template may contain at most {MAX_TEMPLATE_FIELDS} fields"
            ));
        }
        if serde_json::to_vec(&self.data)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_NODE_TEMPLATE_DATA_BYTES
        {
            return Err(format!(
                "Node template data may contain at most {MAX_NODE_TEMPLATE_DATA_BYTES} bytes"
            ));
        }

        let mut keys = std::collections::HashSet::new();

        for field in &self.schema {
            let mut chars = field.key.chars();
            let valid_start = chars
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
            let valid_rest = chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');

            if field.key.len() > 64 || !valid_start || !valid_rest {
                return Err(format!("Invalid node template field key: {}", field.key));
            }
            if !keys.insert(&field.key) {
                return Err(format!("Duplicate node template field key: {}", field.key));
            }
            if field
                .label
                .as_ref()
                .is_some_and(|label| label.chars().count() > 255)
            {
                return Err(format!(
                    "Label for node template field {} is too long",
                    field.key
                ));
            }
            if !field.kind.matches_value(&field.default) {
                return Err(format!(
                    "Invalid default value for node template field: {}",
                    field.key
                ));
            }
        }

        Ok(())
    }

    pub fn apply_schema(&self, data: &mut Value) -> Result<(), String> {
        if self.schema.is_empty() {
            return Self::validate_data_size(data);
        }

        let object = match data {
            Value::Null => {
                *data = Value::Object(serde_json::Map::new());
                data.as_object_mut().expect("new JSON object")
            }
            Value::Object(object) => object,
            _ => return Err("Node data must be a JSON object when a template is used".into()),
        };

        for field in &self.schema {
            let value = object
                .entry(field.key.clone())
                .or_insert_with(|| field.default.clone());

            if !field.kind.matches_value(value) {
                return Err(format!(
                    "Invalid value for node template field: {}",
                    field.key
                ));
            }
        }

        Self::validate_data_size(data)
    }

    fn validate_data_size(data: &Value) -> Result<(), String> {
        if serde_json::to_vec(data)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_NODE_TEMPLATE_DATA_BYTES
        {
            return Err(format!(
                "Node data may contain at most {MAX_NODE_TEMPLATE_DATA_BYTES} bytes"
            ));
        }

        Ok(())
    }

    pub fn synchronize_data_with_schema(&mut self) -> Result<(), String> {
        self.validate_schema()?;
        self.name = self.name.trim().to_string();

        if !self.schema.is_empty() {
            self.data = Value::Object(
                self.schema
                    .iter()
                    .map(|field| (field.key.clone(), field.default.clone()))
                    .collect(),
            );
        }

        Ok(())
    }
}

impl ColumnCounter for ContentNodeTemplate {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
pub struct Comment {
    #[ts(as = "i32")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<Entry>,
    #[ts(skip)]
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl FromRow<'_, PgRow> for Comment {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let mut entry = None;

        if let Ok((id, title, r#type, slug)) =
            row.try_get::<(i32, String, String, String), &str>("entry")
        {
            entry = Some(Entry {
                id,
                title,
                r#type,
                slug,
            });
        };

        Ok(Self {
            id: row.try_get("id").ok(),
            entry_id: row.try_get("entry_id").ok(),
            parent_id: row.try_get("parent_id").ok(),
            user_id: row.try_get("user_id").ok(),
            author_name: row.try_get("author_name").ok(),
            author_email: row.try_get("author_email").ok(),
            text: row.try_get("text").ok(),
            status: row.try_get("status").ok(),
            created_at: row.try_get("created_at").ok(),
            updated_at: row.try_get("updated_at").ok(),
            entry,
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for Comment {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
pub struct Entry {
    id: i32,
    title: String,
    r#type: String,
    slug: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "models.d.ts")]
pub struct MailTarget {
    #[serde(default)]
    pub id: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub recipients: Vec<String>,
    #[serde(default)]
    pub allow_html: bool,
    #[ts(skip)]
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl MailTarget {
    pub fn new(recipient: String, allow_html: bool) -> Self {
        Self {
            recipients: vec![recipient],
            allow_html,
            ..Default::default()
        }
    }
}

impl FromRow<'_, PgRow> for MailTarget {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            name: row.try_get("name").unwrap_or_default(),
            subject: row.try_get("subject").ok(),
            recipients: row.try_get("recipients").unwrap_or_default(),
            allow_html: row.try_get("allow_html").unwrap_or_default(),
            total_count: row.try_get("total_count").ok(),
        })
    }
}

impl ColumnCounter for MailTarget {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        ContentNodeDataField, ContentNodeDataKind, ContentNodeTemplate,
        MAX_NODE_TEMPLATE_DATA_BYTES,
    };

    fn template() -> ContentNodeTemplate {
        ContentNodeTemplate {
            name: "settings".into(),
            schema: vec![
                ContentNodeDataField {
                    key: "mainpage".into(),
                    kind: ContentNodeDataKind::Boolean,
                    default: json!(false),
                    ..Default::default()
                },
                ContentNodeDataField {
                    key: "priority".into(),
                    kind: ContentNodeDataKind::Number,
                    default: json!(0),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn template_schema_applies_typed_defaults() {
        let mut data = json!({ "mainpage": true });

        template().apply_schema(&mut data).expect("valid data");

        assert_eq!(data, json!({ "mainpage": true, "priority": 0 }));
    }

    #[test]
    fn template_schema_rejects_invalid_types_and_preserves_extension_fields() {
        let mut invalid_type = json!({ "mainpage": "true", "priority": 0 });
        assert!(template().apply_schema(&mut invalid_type).is_err());

        let mut unknown_field = json!({ "mainpage": true, "priority": 0, "extra": true });
        template()
            .apply_schema(&mut unknown_field)
            .expect("extension fields remain supported");
        assert_eq!(unknown_field["extra"], true);
    }

    #[test]
    fn template_definition_is_bounded_and_synchronizes_defaults() {
        let mut template = template();
        template.name = "  settings  ".into();
        template.data = json!({ "obsolete": true });

        template
            .synchronize_data_with_schema()
            .expect("valid template");

        assert_eq!(template.name, "settings");
        assert_eq!(template.data, json!({ "mainpage": false, "priority": 0 }));

        template.name.clear();
        assert!(template.validate_schema().is_err());

        template.name = "settings".into();
        template.schema = (0..65)
            .map(|index| ContentNodeDataField {
                key: format!("field_{index}"),
                kind: ContentNodeDataKind::Boolean,
                default: json!(false),
                ..Default::default()
            })
            .collect();
        assert!(template.validate_schema().is_err());

        template.schema.clear();
        template.data = json!({ "value": "x".repeat(MAX_NODE_TEMPLATE_DATA_BYTES) });
        assert!(template.validate_schema().is_err());
    }
}
