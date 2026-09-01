CREATE TABLE comment_moderation_tokens (
    id BIGSERIAL PRIMARY KEY,
    comment_id BIGINT NOT NULL REFERENCES comments (id) ON DELETE CASCADE,
    token_hash BYTEA NOT NULL UNIQUE,
    action VARCHAR(8) NOT NULL CHECK (action IN ('approved', 'rejected')),
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX comment_moderation_tokens_pending_idx ON comment_moderation_tokens (comment_id, expires_at)
WHERE
    used_at IS NULL;

ALTER TABLE mail_targets
ADD COLUMN allow_dynamic_recipient BOOLEAN NOT NULL DEFAULT FALSE;
