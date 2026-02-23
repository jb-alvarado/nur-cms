use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use ts_rs::TS;

use super::{ContentAuthorFields, ContentCategoryFields, ContentNodeFields, traits::StrCompare};

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContentEntryFields {
    ID,
    GroupID,
    CategoryID,
    LocaleID,
    MediaID,
    #[default]
    Slug,
    Status,
    Author(ContentAuthorFields),
    Category(ContentCategoryFields),
    Type,
    Tags,
    Meta,
    Title,
    CreatedAt,
    UpdatedAt,
    GroupMembers,
    Media,
    CommentCount,
    Node(ContentNodeFields),
}

impl StrCompare for ContentEntryFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::GroupID => other == "group_id",
            Self::CategoryID => other == "category_id",
            Self::LocaleID => other == "locale_id",
            Self::MediaID => other == "media_id",
            Self::Slug => other == "slug",
            Self::Status => other == "status",
            Self::Author(_) => false,
            Self::Category(_) => false,
            Self::Type => other == "type",
            Self::Tags => other == "tags",
            Self::Meta => other == "meta",
            Self::Title => other == "title",
            Self::CreatedAt => other == "created_at",
            Self::UpdatedAt => other == "updated_at",
            Self::GroupMembers => other == "group_members",
            Self::Media => other == "media",
            Self::CommentCount => other == "comment_count",
            Self::Node(_) => false,
        }
    }
}

impl FromStr for ContentEntryFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        // Handle nested author fields: "author.bio", etc.
        if let Some(author_field) = input.strip_prefix("author.") {
            let author_fields = ContentAuthorFields::from_str(author_field)?;
            return Ok(Self::Author(author_fields));
        }
        // Handle nested category fields: "category.name", etc.
        if let Some(category_field) = input.strip_prefix("category.") {
            let category_fields = ContentCategoryFields::from_str(category_field)?;
            return Ok(Self::Category(category_fields));
        }
        // Handle nested node fields: "node.text", "node.data", etc.
        if let Some(node_field) = input.strip_prefix("node.") {
            let node_fields = ContentNodeFields::from_str(node_field)?;
            return Ok(Self::Node(node_fields));
        }

        match input {
            "id" => Ok(Self::ID),
            "group_id" => Ok(Self::GroupID),
            "category_id" => Ok(Self::CategoryID),
            "locale_id" => Ok(Self::LocaleID),
            "media_id" => Ok(Self::MediaID),
            "slug" => Ok(Self::Slug),
            "status" => Ok(Self::Status),
            "type" => Ok(Self::Type),
            "tags" => Ok(Self::Tags),
            "meta" => Ok(Self::Meta),
            "title" => Ok(Self::Title),
            "created_at" => Ok(Self::CreatedAt),
            "updated_at" => Ok(Self::UpdatedAt),
            "group_members" => Ok(Self::GroupMembers),
            "media" => Ok(Self::Media),
            "comment_count" => Ok(Self::CommentCount),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for ContentEntryFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::GroupID => write!(f, "group_id"),
            Self::CategoryID => write!(f, "category_id"),
            Self::LocaleID => write!(f, "locale_id"),
            Self::MediaID => write!(f, "media_id"),
            Self::Slug => write!(f, "slug"),
            Self::Status => write!(f, "status"),
            Self::Author(ref author_field) => write!(f, "author.{}", author_field),
            Self::Category(ref category_field) => write!(f, "category.{}", category_field),
            Self::Type => write!(f, "type"),
            Self::Tags => write!(f, "tags"),
            Self::Meta => write!(f, "meta"),
            Self::Title => write!(f, "title"),
            Self::CreatedAt => write!(f, "created_at"),
            Self::UpdatedAt => write!(f, "updated_at"),
            Self::GroupMembers => write!(f, "group_members"),
            Self::Media => write!(f, "media"),
            Self::CommentCount => write!(f, "comment_count"),
            Self::Node(ref node_field) => write!(f, "node.{}", node_field),
        }
    }
}
