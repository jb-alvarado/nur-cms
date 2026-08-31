use std::{collections::BTreeMap, fs, path::PathBuf, time::Instant};

use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

use crate::{
    Error,
    manifest::{InstalledPlugin, contained_path, schema_name},
};

#[derive(Debug)]
struct Migration {
    version: i64,
    description: String,
    sql: String,
    checksum: Vec<u8>,
}

pub async fn migrate_plugin(pool: &PgPool, plugin: &InstalledPlugin) -> Result<(), Error> {
    let id = &plugin.manifest.plugin.id;
    let schema = schema_name(id);
    ensure_registry(pool, plugin, &schema).await?;

    let migrations = read_migrations(plugin)?;
    validate_applied_migrations(pool, id, &migrations).await?;
    for migration in migrations.values() {
        apply_migration(pool, id, &schema, migration).await?;
    }

    sqlx::query(
        "UPDATE public.plugin_registry SET version = $2, api_version = $3, manifest_checksum = $4, \
         updated_at = now() WHERE plugin_id = $1",
    )
    .bind(id)
    .bind(&plugin.manifest.plugin.version)
    .bind(i32::try_from(plugin.manifest.plugin.api_version).map_err(|_| Error::InvalidValue)?)
    .bind(&plugin.manifest_checksum)
    .execute(pool)
    .await?;

    Ok(())
}

async fn ensure_registry(
    pool: &PgPool,
    plugin: &InstalledPlugin,
    schema: &str,
) -> Result<(), Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&plugin.manifest.plugin.id)
        .execute(&mut *transaction)
        .await?;
    if let Some(row) =
        sqlx::query("SELECT schema_name FROM public.plugin_registry WHERE plugin_id = $1")
            .bind(&plugin.manifest.plugin.id)
            .fetch_optional(&mut *transaction)
            .await?
    {
        let stored_schema: String = row.try_get("schema_name")?;
        if stored_schema != schema {
            return Err(Error::Migration(format!(
                "plugin '{}' schema changed from '{stored_schema}' to '{schema}'",
                plugin.manifest.plugin.id
            )));
        }
    } else {
        sqlx::query(
            "INSERT INTO public.plugin_registry \
             (plugin_id, version, api_version, schema_name, manifest_checksum) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&plugin.manifest.plugin.id)
        .bind(&plugin.manifest.plugin.version)
        .bind(i32::try_from(plugin.manifest.plugin.api_version).map_err(|_| Error::InvalidValue)?)
        .bind(schema)
        .bind(&plugin.manifest_checksum)
        .execute(&mut *transaction)
        .await?;
    }

    let statement = format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\"");
    // `schema` is derived exclusively from the strictly validated plugin ID.
    sqlx::query(sqlx::AssertSqlSafe(statement))
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

fn read_migrations(plugin: &InstalledPlugin) -> Result<BTreeMap<i64, Migration>, Error> {
    let Some(directory) = &plugin.manifest.migrations.directory else {
        return Ok(BTreeMap::new());
    };
    let directory = contained_path(&plugin.root, directory, "migration directory")?;
    if !directory.is_dir() {
        return Err(Error::Migration(format!(
            "plugin '{}' migration path is not a directory",
            plugin.manifest.plugin.id
        )));
    }

    let mut migrations = BTreeMap::new();
    for entry in fs::read_dir(directory).map_err(Error::Io)? {
        let path = entry.map_err(Error::Io)?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("sql") {
            continue;
        }
        let migration = parse_migration(path)?;
        if migrations.insert(migration.version, migration).is_some() {
            return Err(Error::Migration(format!(
                "plugin '{}' contains duplicate migration versions",
                plugin.manifest.plugin.id
            )));
        }
        if migrations.len() > 256 {
            return Err(Error::Migration(format!(
                "plugin '{}' contains more than 256 migrations",
                plugin.manifest.plugin.id
            )));
        }
    }
    Ok(migrations)
}

fn parse_migration(path: PathBuf) -> Result<Migration, Error> {
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| Error::Migration("migration filename is not valid UTF-8".into()))?;
    let stem = filename
        .strip_suffix(".sql")
        .ok_or_else(|| Error::Migration(format!("invalid migration filename '{filename}'")))?;
    let (version, description) = stem
        .split_once('_')
        .ok_or_else(|| Error::Migration(format!("invalid migration filename '{filename}'")))?;
    let version = version
        .parse::<i64>()
        .ok()
        .filter(|version| *version > 0)
        .ok_or_else(|| Error::Migration(format!("invalid migration version in '{filename}'")))?;
    if description.is_empty() || description.len() > 255 {
        return Err(Error::Migration(format!(
            "invalid migration description in '{filename}'"
        )));
    }
    if fs::metadata(&path).map_err(Error::Io)?.len() > 8 * 1024 * 1024 {
        return Err(Error::Migration(format!(
            "migration '{filename}' exceeds 8 MiB"
        )));
    }
    let bytes = fs::read(&path).map_err(Error::Io)?;
    let sql = String::from_utf8(bytes.clone())
        .map_err(|error| Error::Migration(format!("{filename}: {error}")))?;
    Ok(Migration {
        version,
        description: description.replace('_', " "),
        sql,
        checksum: Sha256::digest(bytes).to_vec(),
    })
}

async fn validate_applied_migrations(
    pool: &PgPool,
    plugin_id: &str,
    migrations: &BTreeMap<i64, Migration>,
) -> Result<(), Error> {
    let rows = sqlx::query(
        "SELECT version, checksum FROM public._plugin_migrations \
         WHERE plugin_id = $1 ORDER BY version",
    )
    .bind(plugin_id)
    .fetch_all(pool)
    .await?;
    for row in rows {
        let version: i64 = row.try_get("version")?;
        let checksum: Vec<u8> = row.try_get("checksum")?;
        let Some(migration) = migrations.get(&version) else {
            return Err(Error::Migration(format!(
                "plugin '{plugin_id}' no longer contains applied migration {version}"
            )));
        };
        if checksum != migration.checksum {
            return Err(Error::Migration(format!(
                "plugin '{plugin_id}' changed applied migration {version}"
            )));
        }
    }
    Ok(())
}

async fn apply_migration(
    pool: &PgPool,
    plugin_id: &str,
    schema: &str,
    migration: &Migration,
) -> Result<(), Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(plugin_id)
        .execute(&mut *transaction)
        .await?;
    if let Some(row) = sqlx::query(
        "SELECT checksum FROM public._plugin_migrations WHERE plugin_id = $1 AND version = $2",
    )
    .bind(plugin_id)
    .bind(migration.version)
    .fetch_optional(&mut *transaction)
    .await?
    {
        let checksum: Vec<u8> = row.try_get("checksum")?;
        if checksum != migration.checksum {
            return Err(Error::Migration(format!(
                "plugin '{plugin_id}' changed applied migration {}",
                migration.version
            )));
        }
        transaction.commit().await?;
        return Ok(());
    }

    let search_path = format!("SET LOCAL search_path TO \"{schema}\", pg_temp");
    // `schema` is derived exclusively from the strictly validated plugin ID.
    sqlx::query(sqlx::AssertSqlSafe(search_path))
        .execute(&mut *transaction)
        .await?;
    let started = Instant::now();
    sqlx::raw_sql(sqlx::AssertSqlSafe(migration.sql.clone()))
        .execute(&mut *transaction)
        .await?;
    let elapsed = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    sqlx::query(
        "INSERT INTO public._plugin_migrations \
         (plugin_id, version, description, checksum, execution_time_ms) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(plugin_id)
    .bind(migration.version)
    .bind(&migration.description)
    .bind(&migration.checksum)
    .bind(elapsed)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{env, path::PathBuf};

    use sqlx::PgPool;

    use super::{migrate_plugin, parse_migration};
    use crate::manifest::{
        InstalledPlugin, Manifest, MigrationManifest, PluginManifest, schema_name,
    };

    #[test]
    fn rejects_migration_without_numeric_prefix() {
        assert!(parse_migration(PathBuf::from("invalid.sql")).is_err());
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL via DATABASE_URL"]
    async fn applies_plugin_migrations_once_and_records_them() {
        let database_url = env::var("DATABASE_URL").expect("DATABASE_URL is configured");
        let pool = PgPool::connect(&database_url)
            .await
            .expect("test database is available");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("core migrations apply");

        let plugin_id = "ci-echo";
        let schema = schema_name(plugin_id);
        remove_test_plugin(&pool, plugin_id, &schema).await;
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/echo");
        let plugin = InstalledPlugin {
            manifest: Manifest {
                plugin: PluginManifest {
                    id: plugin_id.into(),
                    version: "0.1.0".into(),
                    api_version: 1,
                    cms_version: ">=0.16, <0.17".into(),
                    module: "unused.wasm".into(),
                },
                migrations: MigrationManifest {
                    directory: Some("migrations".into()),
                },
                routes: Vec::new(),
                assets: None,
                cache: None,
                admin: None,
            },
            module: root.join("unused.wasm"),
            root,
            assets: None,
            manifest_checksum: vec![1, 2, 3],
        };

        migrate_plugin(&pool, &plugin)
            .await
            .expect("plugin migration applies");
        migrate_plugin(&pool, &plugin)
            .await
            .expect("reapplying plugin migration is idempotent");

        let migration_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public._plugin_migrations WHERE plugin_id = $1",
        )
        .bind(plugin_id)
        .fetch_one(&pool)
        .await
        .expect("migration record can be read");
        let table_name = format!("{schema}.echo_messages");
        let migrated_table: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::text")
            .bind(table_name)
            .fetch_one(&pool)
            .await
            .expect("plugin table can be inspected");

        assert_eq!(migration_count, 1);
        assert!(migrated_table.is_some());
        remove_test_plugin(&pool, plugin_id, &schema).await;
    }

    async fn remove_test_plugin(pool: &PgPool, plugin_id: &str, schema: &str) {
        sqlx::query("DELETE FROM public.plugin_registry WHERE plugin_id = $1")
            .bind(plugin_id)
            .execute(pool)
            .await
            .expect("test plugin registry entry can be removed");
        let drop_schema = format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE");
        sqlx::query(sqlx::AssertSqlSafe(drop_schema))
            .execute(pool)
            .await
            .expect("test plugin schema can be removed");
    }
}
