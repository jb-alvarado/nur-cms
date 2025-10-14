use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum AuthUserFields {
    ID,
    #[default]
    Email,
    Username,
    Password,
    CreatedAt,
    UpdatedAt,
    LastLogin,
    Role,
}

impl StrCompare for AuthUserFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::Email => other == "email",
            Self::Username => other == "username",
            Self::Password => other == "password",
            Self::CreatedAt => other == "created_at",
            Self::UpdatedAt => other == "updated_at",
            Self::LastLogin => other == "last_login",
            Self::Role => other == "role",
        }
    }
}

impl FromStr for AuthUserFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "email" => Ok(Self::Email),
            "username" => Ok(Self::Username),
            "password" => Ok(Self::Password),
            "created_at" => Ok(Self::CreatedAt),
            "updated_at" => Ok(Self::UpdatedAt),
            "last_login" => Ok(Self::LastLogin),
            "role" => Ok(Self::Role),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for AuthUserFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::Email => write!(f, "email"),
            Self::Username => write!(f, "username"),
            Self::Password => write!(f, "password"),
            Self::CreatedAt => write!(f, "created_at"),
            Self::UpdatedAt => write!(f, "updated_at"),
            Self::LastLogin => write!(f, "last_login"),
            Self::Role => write!(f, "role"),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum LocaleFields {
    ID,
    #[default]
    Code,
    Name,
}

impl StrCompare for LocaleFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::Code => other == "code",
            Self::Name => other == "name",
        }
    }
}

impl FromStr for LocaleFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "code" => Ok(Self::Code),
            "name" => Ok(Self::Name),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for LocaleFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::Code => write!(f, "code"),
            Self::Name => write!(f, "name"),
        }
    }
}

pub trait StrCompare {
    fn is_equal_to_str(&self, other: &str) -> bool;
}

pub trait ColumnCounter {
    fn total_count(&self) -> i64;
}
