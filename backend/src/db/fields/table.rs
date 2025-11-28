use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum Table {
    AuthRoles,
    AuthUsers,
    Locales,
    ContentAuthors,
    ContentEntryAuthors,
    ContentTypes,
    ContentCategories,
    ContentTags,
    ContentEntryTags,
    ContentMeta,
    ContentBlocks,
    ContentEntries,
    Media,
    MediaVariants,
    TsConfig,
}

impl FromStr for Table {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "auth_roles" => Ok(Self::AuthRoles),
            "auth_users" => Ok(Self::AuthUsers),
            "locales" => Ok(Self::Locales),
            "content_authors" => Ok(Self::ContentAuthors),
            "content_entry_authors" => Ok(Self::ContentEntryAuthors),
            "content_types" => Ok(Self::ContentTypes),
            "content_categories" => Ok(Self::ContentCategories),
            "content_tags" => Ok(Self::ContentTags),
            "content_entry_tags" => Ok(Self::ContentEntryTags),
            "content_meta" => Ok(Self::ContentMeta),
            "content_blocks" => Ok(Self::ContentBlocks),
            "content_entries" => Ok(Self::ContentEntries),
            "media" => Ok(Self::Media),
            "media_variants" => Ok(Self::MediaVariants),
            "ts_config" => Ok(Self::TsConfig),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::AuthRoles => write!(f, "auth_roles"),
            Self::AuthUsers => write!(f, "auth_users"),
            Self::Locales => write!(f, "locales"),
            Self::ContentAuthors => write!(f, "content_authors"),
            Self::ContentEntryAuthors => write!(f, "content_entry_authors"),
            Self::ContentTypes => write!(f, "content_types"),
            Self::ContentCategories => write!(f, "content_categories"),
            Self::ContentTags => write!(f, "content_tags"),
            Self::ContentEntryTags => write!(f, "content_entry_tags"),
            Self::ContentMeta => write!(f, "content_meta"),
            Self::ContentBlocks => write!(f, "content_blocks"),
            Self::ContentEntries => write!(f, "content_entries"),
            Self::Media => write!(f, "media"),
            Self::MediaVariants => write!(f, "media_variants"),
            Self::TsConfig => write!(f, "pg_catalog.pg_ts_config"),
        }
    }
}
