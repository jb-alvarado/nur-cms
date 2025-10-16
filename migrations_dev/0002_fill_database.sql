INSERT INTO content_types (id, name, slug, description)
VALUES
  (1, 'BlogPost', 'blog-post', 'A blog article entry'),
  (2, 'Page', 'page', 'A static page'),
  (3, 'Event', 'event', 'An event entry');

INSERT INTO content_fields (content_type_id, name, field_type, required, order_index)
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

INSERT INTO content_items (content_type_id, slug, status, created_by, updated_by)
VALUES
  (1, 'first-blog-post', 'published', 1, 1),
  (2, 'about-page', 'published', 1, 1),
  (3, 'rust-workshop', 'draft', 1, 1);

INSERT INTO content_values (content_item_id, field_id, locale_id, value)
VALUES
  -- title
  (1, (SELECT id FROM content_fields WHERE content_type_id = 1 AND name = 'title'), 1, '"Welcome to my blog"'),
  -- body
  (1, (SELECT id FROM content_fields WHERE content_type_id = 1 AND name = 'body'), 1, '"# Hello World\nThis is my first blog post in **Markdown**."'),
  -- published_at
  (1, (SELECT id FROM content_fields WHERE content_type_id = 1 AND name = 'published_at'), 1, '"2025-10-16T12:00:00Z"'),

  -- Page
  -- title
  (2, (SELECT id FROM content_fields WHERE content_type_id = 2 AND name = 'title'), 1, '"About Us"'),
  -- body
  (2, (SELECT id FROM content_fields WHERE content_type_id = 2 AND name = 'body'), 1, '"This is the **about** page written in Markdown."'),

  -- Event
  -- title
  (3, (SELECT id FROM content_fields WHERE content_type_id = 3 AND name = 'title'), 1, '"Rust Workshop"'),
  -- description
  (3, (SELECT id FROM content_fields WHERE content_type_id = 3 AND name = 'description'), 1, '"Learn Rust in a **hands-on** workshop!"'),
  -- date
  (3, (SELECT id FROM content_fields WHERE content_type_id = 3 AND name = 'date'), 1, '"2025-11-01T09:00:00Z"'),
  -- location
  (3, (SELECT id FROM content_fields WHERE content_type_id = 3 AND name = 'location'), 1, '"Berlin, Germany"'),
  -- is_online
  (3, (SELECT id FROM content_fields WHERE content_type_id = 3 AND name = 'is_online'), 1, 'false');
