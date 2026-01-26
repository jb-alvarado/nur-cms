use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use ts_rs::TS;

use super::traits::StrCompare;

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
