CREATE TABLE plugin_registry (
    plugin_id VARCHAR(40) PRIMARY KEY,
    version VARCHAR(64) NOT NULL,
    api_version INTEGER NOT NULL,
    schema_name VARCHAR(63) NOT NULL UNIQUE,
    manifest_checksum BYTEA NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE _plugin_migrations (
    plugin_id VARCHAR(40) NOT NULL REFERENCES plugin_registry (plugin_id) ON DELETE CASCADE,
    version BIGINT NOT NULL,
    description VARCHAR(255) NOT NULL,
    checksum BYTEA NOT NULL,
    execution_time_ms BIGINT NOT NULL,
    installed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (plugin_id, version)
);
