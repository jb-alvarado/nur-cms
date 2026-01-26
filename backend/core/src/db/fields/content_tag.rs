use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use ts_rs::TS;

use super::traits::StrCompare;

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
