CREATE TABLE media_subtitles (
    id UUID PRIMARY KEY,
    media_id INT NOT NULL REFERENCES media (id) ON DELETE CASCADE,
    language VARCHAR(35) NOT NULL,
    label VARCHAR(100) NOT NULL,
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    webvtt TEXT NOT NULL,
    UNIQUE (media_id, language),
    CHECK (octet_length(webvtt) <= 524288)
);

CREATE UNIQUE INDEX media_subtitles_one_default ON media_subtitles (media_id)
WHERE
    is_default;

-- Durable publication intents bridge the database commit and filesystem rename.
CREATE TABLE media_subtitle_publications (
    media_id INT PRIMARY KEY REFERENCES media (id) ON DELETE CASCADE,
    previous_master TEXT NOT NULL,
    master TEXT NOT NULL,
    backup_filename TEXT NOT NULL
);
