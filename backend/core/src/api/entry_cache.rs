use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::{
    body::Bytes,
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use moka::sync::Cache;
use serde::Serialize;

use crate::{env_bounded_i64, utils::errors::NurError};

/// Process-local cache for public content entry responses.
///
/// The generation is part of every key so a response started before an
/// invalidation cannot repopulate the current cache with stale data.
#[derive(Clone)]
pub struct EntryCache {
    responses: Cache<String, Bytes>,
    generation: Arc<AtomicU64>,
    enabled: bool,
}

impl EntryCache {
    pub fn from_env() -> Self {
        let enabled = std::env::var("NUR_ENTRY_CACHE")
            .map(|value| value == "1")
            .unwrap_or(true);
        let capacity = env_bounded_i64("NUR_ENTRY_CACHE_CAPACITY", 512, 16, 100_000) as u64;
        let time_to_idle = env_bounded_i64("NUR_ENTRY_CACHE_TTI_SECONDS", 1_800, 30, 86_400);
        let time_to_live =
            env_bounded_i64("NUR_ENTRY_CACHE_TTL_SECONDS", 86_400, 30, 604_800).max(time_to_idle);

        Self::new(enabled, capacity, time_to_idle as u64, time_to_live as u64)
    }

    fn new(enabled: bool, capacity: u64, time_to_idle: u64, time_to_live: u64) -> Self {
        Self {
            responses: Cache::builder()
                .max_capacity(capacity)
                .time_to_idle(Duration::from_secs(time_to_idle))
                .time_to_live(Duration::from_secs(time_to_live))
                .build(),
            generation: Arc::new(AtomicU64::new(0)),
            enabled,
        }
    }

    pub fn entry_key(&self, uri: &str, output: &str) -> String {
        format!("{}:{output}:{uri}", self.generation.load(Ordering::Acquire))
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn get(&self, key: &str) -> Option<Bytes> {
        self.enabled.then(|| self.responses.get(key)).flatten()
    }

    pub fn insert(&self, key: String, response: Bytes) {
        if self.enabled {
            self.responses.insert(key, response);
        }
    }

    pub fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.responses.invalidate_all();
    }
}

pub fn encode_json<T: Serialize>(value: &T) -> Result<Bytes, NurError> {
    Ok(serde_json::to_vec(value)?.into())
}

pub fn json_response(body: Bytes) -> Response {
    ([(CONTENT_TYPE, "application/json")], body).into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Bytes;

    use super::EntryCache;

    #[test]
    fn invalidation_changes_cache_keys() {
        let cache = EntryCache::new(true, 16, 30, 60);
        let before = cache.entry_key("/content/entries/note/example?fields=title", "ast");

        cache.invalidate();

        let after = cache.entry_key("/content/entries/note/example?fields=title", "ast");
        assert_ne!(before, after);
    }

    #[test]
    fn invalidation_removes_cached_responses() {
        let cache = EntryCache::new(true, 16, 30, 60);
        let key = cache.entry_key("/content/entries?fields=title", "ast");
        cache.insert(key.clone(), Bytes::from_static(b"{}"));
        assert!(cache.get(&key).is_some());

        cache.invalidate();
        assert!(cache.get(&key).is_none());
    }
}
