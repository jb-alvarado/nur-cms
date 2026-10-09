use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS, sqlx::Type)]
#[sqlx(type_name = "VARCHAR")]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum OutputType {
    #[default]
    AST,
    HTML,
    Markdown,
}

impl<'de> Deserialize<'de> for OutputType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "ast" => Ok(Self::AST),
            "html" => Ok(Self::HTML),
            "markdown" => Ok(Self::Markdown),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["AST", "HTML", "Markdown"],
            )),
        }
    }
}

impl FromStr for OutputType {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "ast" => Ok(Self::AST),
            "html" => Ok(Self::HTML),
            "markdown" => Ok(Self::Markdown),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}
