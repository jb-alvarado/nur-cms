use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize};
use strum_macros::EnumIter;
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum Table {
    AuthRoles,
    AuthUsers,
    Locales,
    ContentAuthors,
    ContentEntryAuthors,
    ContentTypes,
    ContentCategories,
    ContentTags,
    ContentEntryTags,
    ContentMeta,
    ContentBlocks,
    ContentEntries,
    Media,
    MediaVariants,
    TsConfig,
}

impl FromStr for Table {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "auth_roles" => Ok(Self::AuthRoles),
            "auth_users" => Ok(Self::AuthUsers),
            "locales" => Ok(Self::Locales),
            "content_authors" => Ok(Self::ContentAuthors),
            "content_entry_authors" => Ok(Self::ContentEntryAuthors),
            "content_types" => Ok(Self::ContentTypes),
            "content_categories" => Ok(Self::ContentCategories),
            "content_tags" => Ok(Self::ContentTags),
            "content_entry_tags" => Ok(Self::ContentEntryTags),
            "content_meta" => Ok(Self::ContentMeta),
            "content_blocks" => Ok(Self::ContentBlocks),
            "content_entries" => Ok(Self::ContentEntries),
            "media" => Ok(Self::Media),
            "media_variants" => Ok(Self::MediaVariants),
            "ts_config" => Ok(Self::TsConfig),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::AuthRoles => write!(f, "auth_roles"),
            Self::AuthUsers => write!(f, "auth_users"),
            Self::Locales => write!(f, "locales"),
            Self::ContentAuthors => write!(f, "content_authors"),
            Self::ContentEntryAuthors => write!(f, "content_entry_authors"),
            Self::ContentTypes => write!(f, "content_types"),
            Self::ContentCategories => write!(f, "content_categories"),
            Self::ContentTags => write!(f, "content_tags"),
            Self::ContentEntryTags => write!(f, "content_entry_tags"),
            Self::ContentMeta => write!(f, "content_meta"),
            Self::ContentBlocks => write!(f, "content_blocks"),
            Self::ContentEntries => write!(f, "content_entries"),
            Self::Media => write!(f, "media"),
            Self::MediaVariants => write!(f, "media_variants"),
            Self::TsConfig => write!(f, "pg_catalog.pg_ts_config"),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS, sqlx::Type)]
#[sqlx(type_name = "VARCHAR")]
#[sqlx(rename_all = "lowercase")]
pub enum OutputType {
    #[default]
    AST,
    HTML,
    Markdown,
}

impl<'de> Deserialize<'de> for OutputType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "ast" => Ok(OutputType::AST),
            "html" => Ok(OutputType::HTML),
            "markdown" => Ok(OutputType::Markdown),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["AST", "HTML", "Markdown"],
            )),
        }
    }
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

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "lowercase")]
pub enum TSLanguage {
    CFGname,
}

impl StrCompare for TSLanguage {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::CFGname => other == "cfgname",
        }
    }
}

impl FromStr for TSLanguage {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "cfgname" => Ok(Self::CFGname),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for TSLanguage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::CFGname => write!(f, "cfgname"),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
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

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum AuthUserFields {
    ID,
    #[default]
    Email,
    Username,
    FirstName,
    LastName,
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
            Self::LastName => other == "last_name",
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
            "last_name" => Ok(Self::LastName),
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
            Self::LastName => write!(f, "last_name"),
            Self::Password => write!(f, "password"),
            Self::CreatedAt => write!(f, "created_at"),
            Self::UpdatedAt => write!(f, "updated_at"),
            Self::LastLogin => write!(f, "last_login"),
            Self::Role => write!(f, "role"),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
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

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContentTypeFields {
    ID,
    #[default]
    Name,
    Slug,
}

impl StrCompare for ContentTypeFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::Name => other == "name",
            Self::Slug => other == "slug",
        }
    }
}

impl FromStr for ContentTypeFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "name" => Ok(Self::Name),
            "slug" => Ok(Self::Slug),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for ContentTypeFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::Name => write!(f, "name"),
            Self::Slug => write!(f, "slug"),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContentCategoryFields {
    ID,
    GroupID,
    LocaleID,
    #[default]
    Name,
    Slug,
    Status,
    MediaID,
    Media,
    GroupMembers,
}

impl StrCompare for ContentCategoryFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::GroupID => other == "group_id",
            Self::LocaleID => other == "locale_id",
            Self::Name => other == "name",
            Self::Slug => other == "slug",
            Self::Status => other == "status",
            Self::MediaID => other == "media_id",
            Self::Media => other == "media",
            Self::GroupMembers => other == "group_members",
        }
    }
}

impl FromStr for ContentCategoryFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "group_id" => Ok(Self::GroupID),
            "locale_id" => Ok(Self::LocaleID),
            "name" => Ok(Self::Name),
            "slug" => Ok(Self::Slug),
            "status" => Ok(Self::Status),
            "media_id" => Ok(Self::MediaID),
            "media" => Ok(Self::Media),
            "group_members" => Ok(Self::GroupMembers),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for ContentCategoryFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::GroupID => write!(f, "group_id"),
            Self::LocaleID => write!(f, "locale_id"),
            Self::Name => write!(f, "name"),
            Self::Slug => write!(f, "slug"),
            Self::Status => write!(f, "status"),
            Self::MediaID => write!(f, "media_id"),
            Self::Media => write!(f, "media"),
            Self::GroupMembers => write!(f, "group_members"),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContentTagFields {
    ID,
    Name,
    #[default]
    Slug,
}

impl StrCompare for ContentTagFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::Name => other == "name",
            Self::Slug => other == "slug",
        }
    }
}

impl FromStr for ContentTagFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "name" => Ok(Self::Name),
            "slug" => Ok(Self::Slug),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for ContentTagFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::Name => write!(f, "name"),
            Self::Slug => write!(f, "slug"),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContentAuthorFields {
    ID,
    #[default]
    FirstName,
    LastName,
    Slug,
    Bio,
    MediaID,
    Media,
}

impl StrCompare for ContentAuthorFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::FirstName => other == "first_name",
            Self::LastName => other == "last_name",
            Self::Slug => other == "slug",
            Self::Bio => other == "bio",
            Self::MediaID => other == "media_id",
            Self::Media => other == "media",
        }
    }
}

impl FromStr for ContentAuthorFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "first_name" => Ok(Self::FirstName),
            "last_name" => Ok(Self::LastName),
            "slug" => Ok(Self::Slug),
            "bio" => Ok(Self::Bio),
            "media_id" => Ok(Self::MediaID),
            "media" => Ok(Self::Media),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for ContentAuthorFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::FirstName => write!(f, "first_name"),
            Self::LastName => write!(f, "last_name"),
            Self::Slug => write!(f, "slug"),
            Self::Bio => write!(f, "bio"),
            Self::MediaID => write!(f, "media_id"),
            Self::Media => write!(f, "media"),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContentFields {
    ID,
    GroupID,
    CategoryID,
    LocaleID,
    MediaID,
    #[default]
    Slug,
    Status,
    Author,
    Category,
    Tags,
    Meta,
    Blocks,
    Title,
    Description,
    Body,
    CreatedAt,
    UpdatedAt,
    GroupMembers,
    Media,
    Embeds,
}

impl StrCompare for ContentFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::GroupID => other == "group_id",
            Self::CategoryID => other == "category_id",
            Self::LocaleID => other == "locale_id",
            Self::MediaID => other == "media_id",
            Self::Slug => other == "slug",
            Self::Status => other == "status",
            Self::Author => other == "author",
            Self::Category => other == "category",
            Self::Tags => other == "tags",
            Self::Meta => other == "meta",
            Self::Blocks => other == "blocks",
            Self::Title => other == "title",
            Self::Description => other == "description",
            Self::Body => other == "body",
            Self::CreatedAt => other == "created_at",
            Self::UpdatedAt => other == "updated_at",
            Self::GroupMembers => other == "group_members",
            Self::Media => other == "media",
            Self::Embeds => other == "embeds",
        }
    }
}

impl FromStr for ContentFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "group_id" => Ok(Self::GroupID),
            "category_id" => Ok(Self::CategoryID),
            "locale_id" => Ok(Self::LocaleID),
            "media_id" => Ok(Self::MediaID),
            "slug" => Ok(Self::Slug),
            "status" => Ok(Self::Status),
            "author" => Ok(Self::Author),
            "category" => Ok(Self::Category),
            "tags" => Ok(Self::Tags),
            "meta" => Ok(Self::Meta),
            "blocks" => Ok(Self::Blocks),
            "title" => Ok(Self::Title),
            "description" => Ok(Self::Description),
            "body" => Ok(Self::Body),
            "created_at" => Ok(Self::CreatedAt),
            "updated_at" => Ok(Self::UpdatedAt),
            "group_members" => Ok(Self::GroupMembers),
            "media" => Ok(Self::Media),
            "embeds" => Ok(Self::Embeds),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for ContentFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::GroupID => write!(f, "group_id"),
            Self::CategoryID => write!(f, "category_id"),
            Self::LocaleID => write!(f, "locale_id"),
            Self::MediaID => write!(f, "media_id"),
            Self::Slug => write!(f, "slug"),
            Self::Status => write!(f, "status"),
            Self::Author => write!(f, "author"),
            Self::Category => write!(f, "category"),
            Self::Tags => write!(f, "tags"),
            Self::Meta => write!(f, "meta"),
            Self::Blocks => write!(f, "blocks"),
            Self::Title => write!(f, "title"),
            Self::Description => write!(f, "description"),
            Self::Body => write!(f, "body"),
            Self::CreatedAt => write!(f, "created_at"),
            Self::UpdatedAt => write!(f, "updated_at"),
            Self::GroupMembers => write!(f, "group_members"),
            Self::Media => write!(f, "media"),
            Self::Embeds => write!(f, "embeds"),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum MediaFields {
    ID,
    Alt,
    #[default]
    Filename,
    Path,
    Type,
    Width,
    Height,
    Size,
    UploadedBy,
    CreatedAt,
    MediaVariants,
}

impl StrCompare for MediaFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::Alt => other == "alt",
            Self::Filename => other == "filename",
            Self::Path => other == "path",
            Self::Type => other == "type",
            Self::Width => other == "width",
            Self::Height => other == "height",
            Self::Size => other == "size",
            Self::UploadedBy => other == "uploaded_by",
            Self::CreatedAt => other == "created_at",
            Self::MediaVariants => other == "media_variants",
        }
    }
}

impl FromStr for MediaFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "alt" => Ok(Self::Alt),
            "filename" => Ok(Self::Filename),
            "path" => Ok(Self::Path),
            "type" => Ok(Self::Type),
            "width" => Ok(Self::Width),
            "height" => Ok(Self::Height),
            "size" => Ok(Self::Size),
            "uploaded_by" => Ok(Self::UploadedBy),
            "created_at" => Ok(Self::CreatedAt),
            "media_variants" => Ok(Self::MediaVariants),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for MediaFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::Alt => write!(f, "alt"),
            Self::Filename => write!(f, "filename"),
            Self::Path => write!(f, "path"),
            Self::Type => write!(f, "type"),
            Self::Width => write!(f, "width"),
            Self::Height => write!(f, "height"),
            Self::Size => write!(f, "size"),
            Self::UploadedBy => write!(f, "uploaded_by"),
            Self::CreatedAt => write!(f, "created_at"),
            Self::MediaVariants => write!(f, "media_variants"),
        }
    }
}

pub trait StrCompare {
    fn is_equal_to_str(&self, other: &str) -> bool;
}

pub trait ColumnCounter {
    fn total_count(&self) -> i64;
}
