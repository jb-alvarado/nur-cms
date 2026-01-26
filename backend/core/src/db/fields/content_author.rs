use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use ts_rs::TS;

use super::traits::StrCompare;

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
