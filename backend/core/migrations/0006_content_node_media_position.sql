ALTER TABLE content_node_media
ADD COLUMN position_index INT;

WITH
    ordered_media AS (
        SELECT
            id,
            (
                row_number() OVER (
                    PARTITION BY
                        node_id
                    ORDER BY
                        ast_line,
                        start_offset NULLS LAST,
                        end_offset NULLS LAST,
                        id
                ) - 1
            )::INT AS position_index
        FROM
            content_node_media
    )
UPDATE content_node_media AS media
SET
    position_index = ordered.position_index
FROM
    ordered_media AS ordered
WHERE
    media.id = ordered.id;

ALTER TABLE content_node_media
ALTER COLUMN position_index
SET NOT NULL,
ADD CONSTRAINT content_node_media_position_index_check CHECK (position_index >= 0),
ADD CONSTRAINT content_node_media_node_position_key UNIQUE (node_id, position_index),
DROP COLUMN ast_line,
DROP COLUMN start_offset,
DROP COLUMN end_offset;
