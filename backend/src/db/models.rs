use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, postgres::PgRow};

use crate::db::fields::ColumnCounter;

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
    pub id: i32,
    pub name: Role,
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
            id: row.get("id"),
            name: role,
        })
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AuthUser {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing)]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_id: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<AuthRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_deserializing, skip_serializing)]
    pub last_login: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing)]
    pub total_count: Option<i64>,
}

impl AuthUser {
    pub fn new(email: String, username: String, password: String, role_id: i32) -> Self {
        Self {
            id: None,
            email: Some(email),
            username: Some(username),
            password: Some(password),
            role_id: Some(role_id),
            role: None,
            created_at: None,
            last_login: None,
            total_count: None,
        }
    }
}

impl FromRow<'_, PgRow> for AuthUser {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let mut role = None;

        if let Ok((id, name)) = row.try_get::<(i32, String), &str>("auth_role") {
            role = Some(AuthRole {
                id,
                name: Role::set_role(&name),
            });
        };

        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            email: row.try_get("email").unwrap_or_default(),
            username: row.try_get("username").unwrap_or_default(),
            password: row.try_get("password").unwrap_or_default(),
            role_id: row.try_get("role_id").unwrap_or_default(),
            role,
            created_at: row.try_get("created_at").unwrap_or_default(),
            last_login: row.try_get("last_login").unwrap_or_default(),
            total_count: row.try_get("total_count").unwrap_or_default(),
        })
    }
}

impl ColumnCounter for AuthUser {
    fn total_count(&self) -> i64 {
        self.total_count.unwrap_or_default()
    }
}
