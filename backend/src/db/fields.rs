use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Table {
    AuthRoles,
    AuthUsers,
    Locales,
    ContentTypes,
    Fields,
    ContentItems,
    ContentValues,
    Media,
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::AuthRoles => write!(f, "auth_roles"),
            Self::AuthUsers => write!(f, "auth_users"),
            Self::Locales => write!(f, "locales"),
            Self::ContentTypes => write!(f, "content_types"),
            Self::Fields => write!(f, "fields"),
            Self::ContentItems => write!(f, "content_items"),
            Self::ContentValues => write!(f, "content_values"),
            Self::Media => write!(f, "media"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TypeSlag {
    BlogPost,
    Event,
    Page,
}

impl fmt::Display for TypeSlag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::BlogPost => write!(f, "blog-post"),
            Self::Event => write!(f, "event"),
            Self::Page => write!(f, "page"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    AST,
    HTML,
    Markdown,
}

impl FromStr for OutputType {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "ast" => Ok(Self::AST),
            "html" => Ok(Self::HTML),
            "markdown" => Ok(Self::Markdown),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum AuthRoleFields {
    ID,
    #[default]
    Name,
}

impl StrCompare for AuthRoleFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::Name => other == "name",
        }
    }
}

impl FromStr for AuthRoleFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "name" => Ok(Self::Name),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for AuthRoleFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::Name => write!(f, "name"),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum AuthUserFields {
    ID,
    #[default]
    Email,
    Username,
    FirstName,
    Lastname,
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
            Self::FirstName => other == "first_name",
            Self::Lastname => other == "last_name",
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
            "first_name" => Ok(Self::FirstName),
            "last_name" => Ok(Self::Lastname),
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
            Self::FirstName => write!(f, "first_name"),
            Self::Lastname => write!(f, "last_name"),
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

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum ContentFields {
    ID,
    #[default]
    Slug,
    Status,
    CreatedAt,
    UpdatedAt,
    Author,
    Title,
    Body,
    Locale,
    Media,
}

impl StrCompare for ContentFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::Slug => other == "slug",
            Self::Status => other == "status",
            Self::CreatedAt => other == "created_at",
            Self::UpdatedAt => other == "updated_at",
            Self::Author => other == "author",
            Self::Title => other == "title",
            Self::Body => other == "body",
            Self::Locale => other == "locale",
            Self::Media => other == "media",
        }
    }
}

impl FromStr for ContentFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "slug" => Ok(Self::Slug),
            "status" => Ok(Self::Status),
            "created_at" => Ok(Self::CreatedAt),
            "updated_at" => Ok(Self::UpdatedAt),
            "author" => Ok(Self::Author),
            "title" => Ok(Self::Title),
            "body" => Ok(Self::Body),
            "locale" => Ok(Self::Locale),
            "media" => Ok(Self::Media),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for ContentFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::Slug => write!(f, "slug"),
            Self::Status => write!(f, "status"),
            Self::CreatedAt => write!(f, "created_at"),
            Self::UpdatedAt => write!(f, "updated_at"),
            Self::Author => write!(f, "author"),
            Self::Title => write!(f, "title"),
            Self::Body => write!(f, "body"),
            Self::Locale => write!(f, "locale"),
            Self::Media => write!(f, "media"),
        }
    }
}

pub trait StrCompare {
    fn is_equal_to_str(&self, other: &str) -> bool;
}

pub trait ColumnCounter {
    fn total_count(&self) -> i64;
}
