use std::{
    collections::HashSet,
    fmt,
    str::FromStr,
    sync::Arc,
    time::{Duration, SystemTime},
};

use tokio::sync::Mutex;
use ts_rs::TS;
use uuid::Uuid;

pub mod routes;

use crate::utils::errors::NurError;

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub struct UuidData {
    pub uuid: Uuid,
    pub expiration: SystemTime,
    pub ip_address: String,
    pub user_id: Option<i32>,
}

impl UuidData {
    pub fn new(ip_address: String, user_id: Option<i32>) -> Self {
        Self {
            uuid: Uuid::new_v4(),
            expiration: SystemTime::now() + Duration::from_mins(30),
            ip_address,
            user_id,
        }
    }
}

impl Default for UuidData {
    fn default() -> Self {
        Self::new(String::from("127.0.0.1"), None)
    }
}

#[derive(Debug, Default, Clone, TS)]
#[ts(export, export_to = "sse.d.ts")]
pub enum SSELevel {
    Error,
    #[default]
    Info,
    Success,
    Warning,
}

impl FromStr for SSELevel {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "error" => Ok(Self::Error),
            "info" => Ok(Self::Info),
            "success" => Ok(Self::Success),
            "warning" => Ok(Self::Warning),
            _ => Err(format!("Field '{input}' not found!")),
        }
    }
}

impl fmt::Display for SSELevel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Error => write!(f, "error"),
            Self::Info => write!(f, "info"),
            Self::Success => write!(f, "success"),
            Self::Warning => write!(f, "warning"),
        }
    }
}

#[derive(Debug, Clone, TS)]
#[ts(export, export_to = "sse.d.ts")]
pub struct SSEMessage {
    pub variance: SSELevel,
    pub text: String,
}

impl SSEMessage {
    pub fn new(variance: SSELevel, text: &str) -> Self {
        Self {
            variance,
            text: text.to_owned(),
        }
    }
}

impl fmt::Display for SSEMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            r#"{{ "variance": "{}", "text": "{}"}}"#,
            self.variance, self.text
        )
    }
}

#[derive(Debug, Clone)]
pub struct SseAuthState {
    pub uuids: Arc<Mutex<HashSet<UuidData>>>,
}

/// Remove all UUIDs from HashSet which are older the expiration time.
pub fn prune_uuids(uuids: &mut HashSet<UuidData>) {
    uuids.retain(|entry| entry.expiration > SystemTime::now());
}

pub fn check_uuid(
    uuids: &mut HashSet<UuidData>,
    uuid: &str,
    ip_address: &str,
) -> Result<&'static str, NurError> {
    let client_uuid = Uuid::parse_str(uuid)
        .map_err(|_| NurError::Forbidden("Invalid missing UUID".to_string()))?;

    prune_uuids(uuids);

    match uuids.iter().find(|entry| entry.uuid == client_uuid) {
        Some(entry) => {
            // Verify IP address matches
            if entry.ip_address != ip_address {
                return Err(NurError::Forbidden("UUID IP address mismatch".to_string()));
            }
            Ok("UUID is valid")
        }
        None => Err(NurError::Forbidden("Invalid or expired UUID".to_string())),
    }
}
