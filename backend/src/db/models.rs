use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, postgres::PgRow};

use crate::db::{fields::ColumnCounter, is_zero};

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
    pub fn new(email: String, username: String, password: String, role_id: i32) -> Self {
        Self {
            email,
            username,
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
