use std::{net::IpAddr, path::PathBuf, sync::Arc, time::Duration};

pub const FORWARDED_REQUEST_HEADERS: &[&str] = &[
    "accept",
    "accept-language",
    "content-type",
    "host",
    "user-agent",
];
pub const TRUSTED_PROXY_REQUEST_HEADERS: &[&str] =
    &["forwarded", "x-forwarded-host", "x-forwarded-proto"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Identity {
    pub user_id: i32,
    pub roles: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub path_params: Vec<(String, String)>,
    pub query: Option<String>,
    pub headers: Vec<Header>,
    pub body: Vec<u8>,
    pub client_ip: Option<IpAddr>,
    pub identity: Option<Identity>,
}

#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<Header>,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct CachePolicy {
    pub ttl: Duration,
    pub max_entries: u64,
    pub vary_headers: Arc<[String]>,
}

#[derive(Clone, Debug)]
pub struct Route {
    pub plugin_id: String,
    pub id: String,
    pub method: String,
    pub path: String,
    pub roles: Vec<String>,
    pub cache: Option<CachePolicy>,
    key: usize,
}

impl Route {
    pub(crate) fn new(
        key: usize,
        plugin_id: String,
        id: String,
        method: String,
        path: String,
        roles: Vec<String>,
        cache: Option<CachePolicy>,
    ) -> Self {
        Self {
            plugin_id,
            id,
            method,
            path,
            roles,
            cache,
            key,
        }
    }

    pub(crate) fn key(&self) -> usize {
        self.key
    }
}

#[derive(Clone, Debug)]
pub struct AssetDirectory {
    pub plugin_id: String,
    pub path: PathBuf,
}
