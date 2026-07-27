CREATE TABLE auth_refresh_tokens (
    jti UUID PRIMARY KEY,
    family_id UUID NOT NULL,
    user_id INTEGER NOT NULL REFERENCES auth_users (id) ON DELETE CASCADE,
    expires_at BIGINT NOT NULL,
    created_at BIGINT NOT NULL,
    revoked_at BIGINT,
    replaced_by UUID
);

CREATE INDEX auth_refresh_tokens_family_id_idx ON auth_refresh_tokens (family_id);
CREATE INDEX auth_refresh_tokens_expires_at_idx ON auth_refresh_tokens (expires_at);
