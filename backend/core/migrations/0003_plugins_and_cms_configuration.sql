-- Plugin infrastructure and instance-wide CMS presentation/feature settings.
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

CREATE TABLE configuration_cms (
    id SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    frontend_name VARCHAR(160) NOT NULL DEFAULT 'NUR CMS',
    logo_media_id INT REFERENCES media(id) ON DELETE SET NULL,
    admin_language VARCHAR(16),
    entry_default_status VARCHAR(16) NOT NULL DEFAULT 'draft',
    entry_hidden_fields TEXT[] NOT NULL DEFAULT '{}',
    hidden_menu_items TEXT[] NOT NULL DEFAULT '{}',
    disabled_features TEXT[] NOT NULL DEFAULT '{}'
);

INSERT INTO configuration_cms (id) VALUES (1)
ON CONFLICT (id) DO NOTHING;

ALTER TABLE content_types
    ADD COLUMN IF NOT EXISTS entry_default_status VARCHAR(16),
    ADD COLUMN IF NOT EXISTS entry_hidden_fields TEXT[] NOT NULL DEFAULT '{}';
