ALTER TABLE media
ADD COLUMN processing_status VARCHAR(16) NOT NULL DEFAULT 'completed'
CHECK (processing_status IN ('queued', 'processing', 'completed', 'failed'));

CREATE TABLE media_processing_jobs (
    id BIGSERIAL PRIMARY KEY,
    media_id INT NOT NULL REFERENCES media (id) ON DELETE CASCADE,
    kind VARCHAR(32) NOT NULL CHECK (kind IN ('video_variants')),
    status VARCHAR(16) NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'running', 'completed', 'failed')),
    attempts INT NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts INT NOT NULL DEFAULT 3 CHECK (max_attempts > 0),
    last_error TEXT,
    locked_at TIMESTAMPTZ,
    lease_expires_at TIMESTAMPTZ,
    lease_token TEXT,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX media_processing_jobs_active_media_kind_idx
ON media_processing_jobs (media_id, kind)
WHERE status IN ('queued', 'running');

CREATE INDEX media_processing_jobs_claim_idx
ON media_processing_jobs (status, created_at)
WHERE status = 'queued';

CREATE INDEX media_processing_jobs_expired_lease_idx
ON media_processing_jobs (lease_expires_at)
WHERE status = 'running';

CREATE TABLE media_video_variants (
    id BIGSERIAL PRIMARY KEY,
    media_id INT NOT NULL REFERENCES media (id) ON DELETE CASCADE,
    kind VARCHAR(16) NOT NULL DEFAULT 'progressive'
        CHECK (kind IN ('progressive', 'hls')),
    profile VARCHAR(64) NOT NULL,
    width INT NOT NULL CHECK (width > 0),
    height INT NOT NULL CHECK (height > 0),
    container VARCHAR(16) NOT NULL,
    video_codec VARCHAR(64) NOT NULL,
    audio_codec VARCHAR(64),
    filename TEXT NOT NULL,
    size BIGINT NOT NULL CHECK (size >= 0),
    duration_ms BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (media_id, kind, profile)
);

CREATE INDEX media_video_variants_media_id_idx ON media_video_variants (media_id);

-- Admin-configurable ffmpeg encoding profiles. Each profile is applied as one
-- independent ffmpeg invocation; `cmd` holds an ordered array of
-- `{"flag": "-c:v", "value": "libx264"}` pairs appended verbatim after the
-- input mapping. `height` gates whether a profile is skipped for sources
-- smaller than its target and is used to auto-inject a `-vf scale=-2:<height>`
-- filter when `cmd` does not already contain a `-vf` entry.
CREATE TABLE video_profiles (
    id SERIAL PRIMARY KEY,
    name VARCHAR(64) NOT NULL UNIQUE,
    container VARCHAR(16) NOT NULL,
    height INT NOT NULL CHECK (height > 0),
    cmd JSONB NOT NULL DEFAULT '[]'::jsonb,
    enabled BOOLEAN NOT NULL DEFAULT true,
    sort_order INT NOT NULL DEFAULT 0
);

INSERT INTO video_profiles (name, container, height, cmd, sort_order) VALUES
('h264-480', 'mp4', 480, '[
    {"flag": "-c:v", "value": "libx264"},
    {"flag": "-crf", "value": "23"},
    {"flag": "-preset", "value": "slower"},
    {"flag": "-pix_fmt", "value": "yuv420p"},
    {"flag": "-c:a", "value": "aac"},
    {"flag": "-b:a", "value": "96k"},
    {"flag": "-ar", "value": "48k"},
    {"flag": "-movflags", "value": "+faststart"}
]'::jsonb, 0),
('h264-720', 'mp4', 720, '[
    {"flag": "-c:v", "value": "libx264"},
    {"flag": "-crf", "value": "23"},
    {"flag": "-preset", "value": "slower"},
    {"flag": "-pix_fmt", "value": "yuv420p"},
    {"flag": "-c:a", "value": "aac"},
    {"flag": "-b:a", "value": "128k"},
    {"flag": "-ar", "value": "48k"},
    {"flag": "-movflags", "value": "+faststart"}
]'::jsonb, 1),
('h264-1080', 'mp4', 1080, '[
    {"flag": "-c:v", "value": "libx264"},
    {"flag": "-crf", "value": "23"},
    {"flag": "-preset", "value": "slower"},
    {"flag": "-pix_fmt", "value": "yuv420p"},
    {"flag": "-c:a", "value": "aac"},
    {"flag": "-b:a", "value": "160k"},
    {"flag": "-ar", "value": "48k"},
    {"flag": "-movflags", "value": "+faststart"}
]'::jsonb, 2);
