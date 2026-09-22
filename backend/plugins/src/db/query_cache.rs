use std::{collections::HashMap, sync::Mutex, time::Duration};

use moka::sync::Cache;
use sha2::{Digest, Sha256};

use super::plugin_database::result_size;
use crate::runtime::bindings::nur::cms::database::{NullType, QueryResult, Statement, Value};

pub(crate) struct PluginDatabaseCache {
    entries: Cache<(u64, [u8; 32]), QueryResult>,
    generations: Mutex<HashMap<String, u64>>,
}

impl PluginDatabaseCache {
    pub(crate) fn new(maximum_size: u64, time_to_live: Duration) -> Self {
        Self {
            entries: Cache::builder()
                .max_capacity(maximum_size)
                .time_to_live(time_to_live)
                .weigher(|_, result: &QueryResult| {
                    u32::try_from(result_size(result).saturating_add(40)).unwrap_or(u32::MAX)
                })
                .build(),
            generations: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn lookup(
        &self,
        schema: &str,
        statement: &Statement,
    ) -> (u64, [u8; 32], Option<QueryResult>) {
        let generation = self.generation(schema);
        let digest = statement_digest(schema, statement);
        let result = self.entries.get(&(generation, digest));
        (generation, digest, result)
    }

    pub(crate) fn insert(
        &self,
        schema: &str,
        generation: u64,
        digest: [u8; 32],
        result: QueryResult,
    ) {
        if self.generation(schema) == generation {
            self.entries.insert((generation, digest), result);
        }
    }

    pub(crate) fn invalidate(&self, schema: &str) {
        let mut generations = self
            .generations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let generation = generations.entry(schema.to_owned()).or_default();
        *generation = generation.wrapping_add(1);
    }

    fn generation(&self, schema: &str) -> u64 {
        *self
            .generations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(schema.to_owned())
            .or_default()
    }
}

fn statement_digest(schema: &str, statement: &Statement) -> [u8; 32] {
    let mut digest = Sha256::new();
    update_bytes(&mut digest, schema.as_bytes());
    update_bytes(&mut digest, statement.sql.as_bytes());
    for value in &statement.params {
        match value {
            Value::Null(kind) => {
                digest.update([0, null_tag(*kind)]);
            }
            Value::Boolean(value) => digest.update([1, u8::from(*value)]),
            Value::Integer(value) => {
                digest.update([2]);
                digest.update(value.to_be_bytes());
            }
            Value::Float(value) => {
                digest.update([3]);
                digest.update(value.to_bits().to_be_bytes());
            }
            Value::Text(value) => {
                digest.update([4]);
                update_bytes(&mut digest, value.as_bytes());
            }
            Value::Bytes(value) => {
                digest.update([5]);
                update_bytes(&mut digest, value);
            }
            Value::Json(value) => {
                digest.update([6]);
                update_bytes(&mut digest, value.as_bytes());
            }
        }
    }
    digest.finalize().into()
}

fn update_bytes(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}

const fn null_tag(kind: NullType) -> u8 {
    match kind {
        NullType::Boolean => 0,
        NullType::Integer => 1,
        NullType::Float => 2,
        NullType::Text => 3,
        NullType::Bytes => 4,
        NullType::Json => 5,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::PluginDatabaseCache;
    use crate::runtime::bindings::nur::cms::database::{QueryResult, Statement, Value};

    #[test]
    fn caches_reads_and_generation_invalidation_hides_old_results() {
        let cache = PluginDatabaseCache::new(1024 * 1024, Duration::from_secs(60));
        let statement = Statement {
            sql: "SELECT value FROM settings WHERE id = $1".into(),
            params: vec![Value::Integer(1)],
        };
        let result = QueryResult {
            rows_affected: 1,
            columns: vec!["value".into()],
            rows: vec![vec![Value::Text("cached".into())]],
        };
        let (generation, digest, missing) = cache.lookup("nur_plugin_test", &statement);
        assert!(missing.is_none());

        cache.insert("nur_plugin_test", generation, digest, result);
        assert!(cache.lookup("nur_plugin_test", &statement).2.is_some());

        cache.invalidate("nur_plugin_test");
        assert!(cache.lookup("nur_plugin_test", &statement).2.is_none());
    }

    #[test]
    fn invalidation_is_scoped_to_one_plugin_schema() {
        let cache = PluginDatabaseCache::new(1024 * 1024, Duration::from_secs(60));
        let statement = Statement {
            sql: "SELECT value FROM settings".into(),
            params: Vec::new(),
        };
        let result = QueryResult {
            rows_affected: 1,
            columns: vec!["value".into()],
            rows: vec![vec![Value::Text("cached".into())]],
        };
        for schema in ["nur_plugin_first", "nur_plugin_second"] {
            let (generation, digest, _) = cache.lookup(schema, &statement);
            cache.insert(schema, generation, digest, result.clone());
        }

        cache.invalidate("nur_plugin_first");

        assert!(cache.lookup("nur_plugin_first", &statement).2.is_none());
        assert!(cache.lookup("nur_plugin_second", &statement).2.is_some());
    }

    #[test]
    fn stale_read_cannot_be_inserted_after_a_concurrent_write() {
        let cache = PluginDatabaseCache::new(1024 * 1024, Duration::from_secs(60));
        let schema = "nur_plugin_test";
        let statement = Statement {
            sql: "SELECT value FROM settings".into(),
            params: Vec::new(),
        };
        let (old_generation, digest, _) = cache.lookup(schema, &statement);

        cache.invalidate(schema);
        cache.insert(
            schema,
            old_generation,
            digest,
            QueryResult {
                rows_affected: 1,
                columns: vec!["value".into()],
                rows: vec![vec![Value::Text("stale".into())]],
            },
        );

        assert!(cache.lookup(schema, &statement).2.is_none());
    }
}
