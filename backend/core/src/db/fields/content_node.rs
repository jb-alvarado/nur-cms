use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use ts_rs::TS;

use super::traits::StrCompare;

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContentNodeFields {
    #[default]
    ID,
    EntryID,
    OrderIndex,
    Blocks,
    #[serde(alias = "ast", alias = "html")]
    Text,
    Data,
    MediaID,
    ParentID,
    Media,
    Embeds,
}

impl StrCompare for ContentNodeFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::EntryID => other == "entry_id",
            Self::OrderIndex => other == "order_index",
            Self::Blocks => other == "blocks",
            Self::Text => other == "text" || other == "ast" || other == "html",
            Self::Data => other == "data",
            Self::MediaID => other == "media_id",
            Self::ParentID => other == "parent_id",
            Self::Media => other == "media",
            Self::Embeds => other == "embeds",
        }
    }
}

impl FromStr for ContentNodeFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "entry_id" => Ok(Self::EntryID),
            "order_index" => Ok(Self::OrderIndex),
            "blocks" => Ok(Self::Blocks),
            "text" => Ok(Self::Text),
            "ast" => Ok(Self::Text),
            "html" => Ok(Self::Text),
            "data" => Ok(Self::Data),
            "media_id" => Ok(Self::MediaID),
            "parent_id" => Ok(Self::ParentID),
            "media" => Ok(Self::Media),
            "embeds" => Ok(Self::Embeds),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for ContentNodeFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::EntryID => write!(f, "entry_id"),
            Self::OrderIndex => write!(f, "order_index"),
            Self::Blocks => write!(f, "blocks"),
            Self::Text => write!(f, "text"),
            Self::Data => write!(f, "data"),
            Self::MediaID => write!(f, "media_id"),
            Self::ParentID => write!(f, "parent_id"),
            Self::Media => write!(f, "media"),
            Self::Embeds => write!(f, "embeds"),
        }
    }
}
