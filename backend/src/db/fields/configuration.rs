use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use ts_rs::TS;

use super::traits::StrCompare;

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, EnumIter, TS)]
#[serde(rename_all = "snake_case")]
pub enum ConfigurationFields {
    #[default]
    ID,
    JwtSecret,
    OutputType,
    MailSmtp,
    MailPort,
    MailUser,
    MailPassword,
    MailStarttls,
    ImageExtensions,
    ImageResolutions,
}

impl StrCompare for ConfigurationFields {
    fn is_equal_to_str(&self, other: &str) -> bool {
        match self {
            Self::ID => other == "id",
            Self::JwtSecret => other == "jwt_secret",
            Self::OutputType => other == "output_type",
            Self::MailSmtp => other == "mail_smtp",
            Self::MailPort => other == "mail_port",
            Self::MailUser => other == "mail_user",
            Self::MailPassword => other == "mail_password",
            Self::MailStarttls => other == "mail_starttls",
            Self::ImageExtensions => other == "image_extensions",
            Self::ImageResolutions => other == "image_resolutions",
        }
    }
}

impl FromStr for ConfigurationFields {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "id" => Ok(Self::ID),
            "jwt_secret" => Ok(Self::JwtSecret),
            "output_type" => Ok(Self::OutputType),
            "mail_smtp" => Ok(Self::MailSmtp),
            "mail_port" => Ok(Self::MailPort),
            "mail_user" => Ok(Self::MailUser),
            "mail_password" => Ok(Self::MailPassword),
            "mail_starttls" => Ok(Self::MailStarttls),
            "image_extensions" => Ok(Self::ImageExtensions),
            "image_resolutions" => Ok(Self::ImageResolutions),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for ConfigurationFields {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ID => write!(f, "id"),
            Self::JwtSecret => write!(f, "jwt_secret"),
            Self::OutputType => write!(f, "output_type"),
            Self::MailSmtp => write!(f, "mail_smtp"),
            Self::MailPort => write!(f, "mail_port"),
            Self::MailUser => write!(f, "mail_user"),
            Self::MailPassword => write!(f, "mail_password"),
            Self::MailStarttls => write!(f, "mail_starttls"),
            Self::ImageExtensions => write!(f, "image_extensions"),
            Self::ImageResolutions => write!(f, "image_resolutions"),
        }
    }
}
