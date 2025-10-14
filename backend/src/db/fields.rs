use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

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

pub trait StrCompare {
    fn is_equal_to_str(&self, other: &str) -> bool;
}

pub trait ColumnCounter {
    fn total_count(&self) -> i64;
}
