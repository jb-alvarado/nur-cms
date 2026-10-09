CREATE TABLE video_settings (
    id INT PRIMARY KEY CHECK (id = 1),
    delivery_mode VARCHAR(8) NOT NULL DEFAULT 'file' CHECK (delivery_mode IN ('file', 'hls'))
);

INSERT INTO
    video_settings (id, delivery_mode)
VALUES
    (1, 'file');

ALTER TABLE media
ADD COLUMN video_delivery_mode VARCHAR(8) NOT NULL DEFAULT 'legacy' CHECK (video_delivery_mode IN ('legacy', 'file', 'hls'));

ALTER TABLE video_profiles
ADD COLUMN hls_enabled BOOLEAN NOT NULL DEFAULT FALSE;

UPDATE video_profiles
SET
    hls_enabled = TRUE
WHERE
    cmd @> '[{"flag":"-c:v","value":"libx264"}]'::jsonb
    AND container = 'mp4'
    AND name IN ('h264-480', 'h264-720', 'h264-1080');

INSERT INTO
    video_profiles (name, container, height, cmd, enabled, hls_enabled, sort_order)
VALUES
    (
        'av1-480',
        'mp4',
        480,
        '[
    {"flag": "-c:v", "value": "libsvtav1"},
    {"flag": "-crf", "value": "32"},
    {"flag": "-preset", "value": "6"},
    {"flag": "-pix_fmt", "value": "yuv420p"}
]'::jsonb,
        FALSE,
        TRUE,
        3
    ),
    (
        'av1-720',
        'mp4',
        720,
        '[
    {"flag": "-c:v", "value": "libsvtav1"},
    {"flag": "-crf", "value": "32"},
    {"flag": "-preset", "value": "6"},
    {"flag": "-pix_fmt", "value": "yuv420p"}
]'::jsonb,
        FALSE,
        TRUE,
        4
    ),
    (
        'av1-1080',
        'mp4',
        1080,
        '[
    {"flag": "-c:v", "value": "libsvtav1"},
    {"flag": "-crf", "value": "32"},
    {"flag": "-preset", "value": "6"},
    {"flag": "-pix_fmt", "value": "yuv420p"}
]'::jsonb,
        FALSE,
        TRUE,
        5
    )
ON CONFLICT (name) DO NOTHING;
