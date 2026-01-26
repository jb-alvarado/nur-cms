use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use ts_rs::TS;

use super::traits::StrCompare;

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum MediaFields {
    ID,
    Alt,
    #[default]
    Filename,
    Path,
    Type,
    Width,
    Height,
    Size,
    UploadedBy,
    CreatedAt,
    MediaVariants,
}

impl StrCompare for MediaFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::Alt => other == "alt",
            Self::Filename => other == "filename",
            Self::Path => other == "path",
            Self::Type => other == "type",
            Self::Width => other == "width",
            Self::Height => other == "height",
            Self::Size => other == "size",
            Self::UploadedBy => other == "uploaded_by",
            Self::CreatedAt => other == "created_at",
            Self::MediaVariants => other == "media_variants",
        }
    }
}

impl FromStr for MediaFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "alt" => Ok(Self::Alt),
            "filename" => Ok(Self::Filename),
            "path" => Ok(Self::Path),
            "type" => Ok(Self::Type),
            "width" => Ok(Self::Width),
            "height" => Ok(Self::Height),
            "size" => Ok(Self::Size),
            "uploaded_by" => Ok(Self::UploadedBy),
            "created_at" => Ok(Self::CreatedAt),
            "media_variants" => Ok(Self::MediaVariants),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for MediaFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::Alt => write!(f, "alt"),
            Self::Filename => write!(f, "filename"),
            Self::Path => write!(f, "path"),
            Self::Type => write!(f, "type"),
            Self::Width => write!(f, "width"),
            Self::Height => write!(f, "height"),
            Self::Size => write!(f, "size"),
            Self::UploadedBy => write!(f, "uploaded_by"),
            Self::CreatedAt => write!(f, "created_at"),
            Self::MediaVariants => write!(f, "media_variants"),
        }
    }
}
