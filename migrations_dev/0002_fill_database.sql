INSERT INTO
    content_types (id, name, slug, description)
VALUES
    (1, 'BlogPost', 'blog-post', 'A blog article entry'),
    (2, 'Page', 'page', 'A static page'),
    (3, 'Event', 'event', 'An event entry');

INSERT INTO
    content_fields (content_type_id, name, field_type, required, order_index)
VALUES
    -- BlogPost
    (1, 'title', 'text', true, 1),
    (1, 'body', 'markdown', true, 2),
    (1, 'published_at', 'datetime', false, 3),
    -- Page
    (2, 'title', 'text', true, 1),
    (2, 'body', 'markdown', true, 2),
    -- Event
    (3, 'title', 'text', true, 1),
    (3, 'description', 'markdown', true, 2),
    (3, 'date', 'datetime', true, 3),
    (3, 'location', 'text', false, 4),
    (3, 'is_online', 'boolean', false, 5);

INSERT INTO
    content_items (content_type_id, slug, status, created_by, updated_by)
VALUES
    (1, 'first-blog-post', 'published', 1, 1),
    (2, 'about-page', 'published', 1, 1),
    (3, 'rust-workshop', 'draft', 1, 1);

INSERT INTO
    content_values (content_item_id, field_id, locale_id, value)
VALUES
    -- title
    (
        1,
        (
            SELECT
                id
            FROM
                content_fields
            WHERE
                content_type_id = 1
                AND name = 'title'
        ),
        2,
        '"Welcome to my blog"'
    ),
    -- body
    (
        1,
        (
            SELECT
                id
            FROM
                content_fields
            WHERE
                content_type_id = 1
                AND name = 'body'
        ),
        2,
        '"# Hello World\nThis is my first blog post in **Markdown**.  \nAnd we have a _second_ paragraph!  \nAnd a picture: ![Cover](/uploads/2025/10/cover.jpg)"'
    ),
    -- published_at
    (
        1,
        (
            SELECT
                id
            FROM
                content_fields
            WHERE
                content_type_id = 1
                AND name = 'published_at'
        ),
        1,
        '"2025-10-16T12:00:00Z"'
    ),
    -- Page
    -- title
    (
        2,
        (
            SELECT
                id
            FROM
                content_fields
            WHERE
                content_type_id = 2
                AND name = 'title'
        ),
        2,
        '"About Us"'
    ),
    -- body
    (
        2,
        (
            SELECT
                id
            FROM
                content_fields
            WHERE
                content_type_id = 2
                AND name = 'body'
        ),
        2,
        '"This is the **about** page written in Markdown."'
    ),
    -- Event
    -- title
    (
        3,
        (
            SELECT
                id
            FROM
                content_fields
            WHERE
                content_type_id = 3
                AND name = 'title'
        ),
        2,
        '"Rust Workshop"'
    ),
    -- description
    (
        3,
        (
            SELECT
                id
            FROM
                content_fields
            WHERE
                content_type_id = 3
                AND name = 'description'
        ),
        2,
        '"Learn Rust in a **hands-on** workshop!"'
    ),
    -- date
    (
        3,
        (
            SELECT
                id
            FROM
                content_fields
            WHERE
                content_type_id = 3
                AND name = 'date'
        ),
        2,
        '"2025-11-01T09:00:00Z"'
    ),
    -- location
    (
        3,
        (
            SELECT
                id
            FROM
                content_fields
            WHERE
                content_type_id = 3
                AND name = 'location'
        ),
        2,
        '"Berlin, Germany"'
    ),
    -- is_online
    (
        3,
        (
            SELECT
                id
            FROM
                content_fields
            WHERE
                content_type_id = 3
                AND name = 'is_online'
        ),
        2,
        'false'
    );

INSERT INTO
    media (alt, filename, path, type, uploaded_by)
VALUES
    ('Cover', 'cover.jpg', '/uploads/2025/10/cover.jpg', 'image', 1);

INSERT INTO
    media_variants (media_id, resolution, format, filename)
VALUES
    (1, 960, 'jpg', 'cover_960.jpg'),
    (1, 480, 'jpg', 'cover_480.jpg'),
    (1, 960, 'avif', 'cover_960.avif'),
    (1, 480, 'avif', 'cover_480.avif'),
    (1, 960, 'webp', 'cover_960.webp'),
    (1, 480, 'webp', 'cover_480.webp');

INSERT INTO
    content_media (content_item_id, media_id, node_index)
VALUES
    (1, 1, 1);
