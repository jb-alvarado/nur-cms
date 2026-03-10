use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use ts_rs::TS;

use super::traits::StrCompare;

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContentTypeFields {
    ID,
    #[default]
    Name,
    Slug,
    OrderIndex,
}

impl StrCompare for ContentTypeFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::Name => other == "name",
            Self::Slug => other == "slug",
            Self::OrderIndex => other == "order_index",
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
            "order_index" => Ok(Self::OrderIndex),
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
            Self::OrderIndex => write!(f, "order_index"),
        }
    }
}
