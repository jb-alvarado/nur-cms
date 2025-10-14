use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, postgres::PgRow};

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

#[derive(Clone, Debug, Default, Deserialize, Serialize, sqlx::FromRow)]
pub struct AuthUser {
    #[serde(skip_deserializing)]
    pub id: i32,
    pub email: String,
    pub username: String,
    #[serde(skip_serializing, default = "String::new")]
    pub password: String,
    pub role_id: i32,
    #[sqlx(default)]
    #[serde(default, skip_deserializing)]
    pub last_login: Option<DateTime<Utc>>,
}

impl AuthUser {
    pub fn new(email: String, username: String, password: String, role_id: i32) -> Self {
        Self {
            id: 0,
            email,
            username,
            password,
            role_id,
            last_login: None,
        }
    }
}
