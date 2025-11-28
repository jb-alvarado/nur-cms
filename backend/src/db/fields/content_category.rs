use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use ts_rs::TS;

use super::traits::StrCompare;

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
