use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use ts_rs::TS;

use super::traits::StrCompare;

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum VideoProfileFields {
    ID,
    #[default]
    Name,
    Container,
    Height,
    Cmd,
    Enabled,
    SortOrder,
}

impl StrCompare for VideoProfileFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::Name => other == "name",
            Self::Container => other == "container",
            Self::Height => other == "height",
            Self::Cmd => other == "cmd",
            Self::Enabled => other == "enabled",
            Self::SortOrder => other == "sort_order",
        }
    }
}

impl FromStr for VideoProfileFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "name" => Ok(Self::Name),
            "container" => Ok(Self::Container),
            "height" => Ok(Self::Height),
            "cmd" => Ok(Self::Cmd),
            "enabled" => Ok(Self::Enabled),
            "sort_order" => Ok(Self::SortOrder),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for VideoProfileFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::Name => write!(f, "name"),
            Self::Container => write!(f, "container"),
            Self::Height => write!(f, "height"),
            Self::Cmd => write!(f, "cmd"),
            Self::Enabled => write!(f, "enabled"),
            Self::SortOrder => write!(f, "sort_order"),
        }
    }
}
