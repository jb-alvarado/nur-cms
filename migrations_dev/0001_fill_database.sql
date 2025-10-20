INSERT INTO
    content_types (id, name, slug)
VALUES
    (1, 'BlogPost', 'blog-post'),
    (2, 'Page', 'page'),
    (3, 'Event', 'event');

INSERT INTO
    content_categories (id, locale_id, name, slug)
VALUES
    (1, 2, 'World Wide IT', 'world-wide-it'),
    (2, 2, 'Newest Rust Projects', 'newest-rust-projects'),
    (3, 2, 'Open Source Meetings', 'open-source-meetings');

INSERT INTO
    content_tags (id, locale_id, name, slug)
VALUES
    (1, 2, 'IT', 'it'),
    (2, 2, 'Rust', 'rust'),
    (3, 2, 'Open Source', 'open-source');

INSERT INTO
    content_entries (id, type_id, locale_id, slug, title, description, text, status, created_by, updated_by)
VALUES
    (1, 1, 2, 'first-blog-post', 'Welcome to my blog', 'This is my first blog post in Markdown.', E'# Hello World\nThis is my first blog post in **Markdown**.\n\nAnd we have a _second_ paragraph!\n\nAnd a inline picture: ![Cover](/uploads/2025/10/cover.jpg)\n\nBut also a block picture:\n\n![Cover Block](/uploads/2025/10/block.jpg)', 'published', 1, 1),
    (2, 2, 2, 'about-us', 'About Us', NULL, E'This is the **about** page written in Markdown.', 'published', 1, 1);

INSERT INTO
    content_entry_categories (entry_id, category_id)
VALUES
    (1, 1);

INSERT INTO
    content_entry_tags (entry_id, tag_id)
VALUES
    (1, 1),
    (1, 2);

INSERT INTO
    media (alt, filename, path, type, uploaded_by)
VALUES
    ('Cover', 'cover.jpg', '/uploads/2025/10/cover.jpg', 'image', 1),
    ('Block', 'block.jpg', '/uploads/2025/10/block.jpg', 'image', 1);

INSERT INTO
    media_variants (media_id, resolution, format, filename)
VALUES
    (1, 960, 'jpg', 'cover_960.jpg'),
    (1, 480, 'jpg', 'cover_480.jpg'),
    (1, 960, 'avif', 'cover_960.avif'),
    (1, 480, 'avif', 'cover_480.avif'),
    (1, 960, 'webp', 'cover_960.webp'),
    (1, 480, 'webp', 'cover_480.webp'),
    (2, 960, 'jpg', 'block_960.jpg'),
    (2, 480, 'jpg', 'block_480.jpg'),
    (2, 960, 'avif', 'block_960.avif'),
    (2, 480, 'avif', 'block_480.avif'),
    (2, 960, 'webp', 'block_960.webp'),
    (2, 480, 'webp', 'block_480.webp');

INSERT INTO
    content_media (entry_id, media_id, ast_line)
VALUES
    (1, 1, 6),
    (1, 2, 10);
