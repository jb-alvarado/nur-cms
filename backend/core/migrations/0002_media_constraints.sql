ALTER TABLE media
ADD CONSTRAINT media_path_filename_key UNIQUE (path, filename);

ALTER TABLE media_variants
ALTER COLUMN media_id
SET NOT NULL;

ALTER TABLE media_variants
ADD CONSTRAINT media_variants_media_dimensions_filename_key UNIQUE (media_id, width, height, filename);
