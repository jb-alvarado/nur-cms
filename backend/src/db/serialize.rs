use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, postgres::PgRow};

use crate::db::{
    fields::ColumnCounter,
    models::{AuthRole, Role},
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AuthUserSerializer {
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
            password: row.try_get("password").ok(),
            role_id: row.try_get("role_id").ok(),
            role,
            created_at: row.try_get("created_at").ok(),
            updated_at: row.try_get("updated_at").ok(),
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
