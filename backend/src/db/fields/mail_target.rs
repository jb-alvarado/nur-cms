use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use ts_rs::TS;

use super::traits::StrCompare;

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum MailTargetFields {
    ID,
    #[default]
    Name,
    Subject,
    Recipients,
    AllowHtml,
}

impl StrCompare for MailTargetFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::Name => other == "name",
            Self::Subject => other == "subject",
            Self::Recipients => other == "recipients",
            Self::AllowHtml => other == "allow_html",
        }
    }
}

impl FromStr for MailTargetFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "name" => Ok(Self::Name),
            "subject" => Ok(Self::Subject),
            "recipients" => Ok(Self::Recipients),
            "allow_html" => Ok(Self::AllowHtml),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for MailTargetFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::Name => write!(f, "name"),
            Self::Subject => write!(f, "subject"),
            Self::Recipients => write!(f, "recipients"),
            Self::AllowHtml => write!(f, "allow_html"),
        }
    }
}
