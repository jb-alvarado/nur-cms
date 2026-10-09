use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use ts_rs::TS;

use super::traits::StrCompare;

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum CommentFields {
    ID,
    EntryID,
    ParentID,
    UserID,
    #[default]
    AuthorName,
    AuthorEmail,
    Text,
    Status,
    CreatedAt,
    UpdatedAt,
    Entry,
}

impl StrCompare for CommentFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::EntryID => other == "entry_id",
            Self::ParentID => other == "parent_id",
            Self::UserID => other == "user_id",
            Self::AuthorName => other == "author_name",
            Self::AuthorEmail => other == "author_email",
            Self::Text => other == "text",
            Self::Status => other == "status",
            Self::CreatedAt => other == "created_at",
            Self::UpdatedAt => other == "updated_at",
            Self::Entry => other == "entry",
        }
    }
}

impl FromStr for CommentFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "entry_id" => Ok(Self::EntryID),
            "parent_id" => Ok(Self::ParentID),
            "user_id" => Ok(Self::UserID),
            "author_name" => Ok(Self::AuthorName),
            "author_email" => Ok(Self::AuthorEmail),
            "text" => Ok(Self::Text),
            "status" => Ok(Self::Status),
            "created_at" => Ok(Self::CreatedAt),
            "updated_at" => Ok(Self::UpdatedAt),
            "entry" => Ok(Self::Entry),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for CommentFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::EntryID => write!(f, "entry_id"),
            Self::ParentID => write!(f, "parent_id"),
            Self::UserID => write!(f, "user_id"),
            Self::AuthorName => write!(f, "author_name"),
            Self::AuthorEmail => write!(f, "author_email"),
            Self::Text => write!(f, "text"),
            Self::Status => write!(f, "status"),
            Self::CreatedAt => write!(f, "created_at"),
            Self::UpdatedAt => write!(f, "updated_at"),
            Self::Entry => write!(f, "entry"),
        }
    }
}
