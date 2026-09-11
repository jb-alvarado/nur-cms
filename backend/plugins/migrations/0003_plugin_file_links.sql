CREATE TABLE IF NOT EXISTS public.plugin_file_links (
    id BIGSERIAL PRIMARY KEY,
    plugin_id VARCHAR(40) NOT NULL REFERENCES public.plugin_registry (plugin_id) ON DELETE CASCADE,
    directory_id VARCHAR(80) NOT NULL,
    purpose VARCHAR(16) NOT NULL CHECK (purpose IN ('upload', 'download')),
    token_hash BYTEA NOT NULL UNIQUE,
    filename VARCHAR(255) NOT NULL,
    max_size BIGINT,
    upload_id VARCHAR(128),
    claimed_at TIMESTAMPTZ,
    finalizing_at TIMESTAMPTZ,
    storage_path VARCHAR(1024),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (max_size IS NULL OR max_size > 0),
    CONSTRAINT plugin_file_links_upload_state_check CHECK (
        (purpose = 'upload' AND max_size IS NOT NULL)
        OR (
            purpose = 'download'
            AND max_size IS NULL
            AND upload_id IS NULL
            AND claimed_at IS NULL
            AND finalizing_at IS NULL
            AND storage_path IS NULL
        )
    )
);

CREATE INDEX IF NOT EXISTS plugin_file_links_active_idx ON public.plugin_file_links (plugin_id, directory_id, expires_at)
WHERE
    consumed_at IS NULL;

CREATE TABLE IF NOT EXISTS public.plugin_storage_directories (
    plugin_id VARCHAR(40) NOT NULL REFERENCES public.plugin_registry (plugin_id) ON DELETE CASCADE,
    directory_id VARCHAR(80) NOT NULL,
    path VARCHAR(512) NOT NULL,
    visibility VARCHAR(16) NOT NULL CHECK (visibility IN ('public', 'private')),
    extensions TEXT[] NOT NULL,
    PRIMARY KEY (plugin_id, directory_id),
    CHECK (cardinality(extensions) > 0)
);
