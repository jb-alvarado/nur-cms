-- First Blog Post (English)
INSERT INTO content_values (content_item_id, field_id, locale_id, value)
VALUES
    (1, 1, 1, '"My First Blog Post"'),
    (1, 2, 1, '"first-blog-post"'),
    (1, 3, 1, '"A short introduction to my first blog post."'),
    (1, 4, 1, '"<p>This is the body of my first blog post, written in HTML format.</p>"'),
    (1, 5, 1, '"2025-10-15T10:00:00Z"');

-- Second Blog Post (English)
INSERT INTO content_values (content_item_id, field_id, locale_id, value)
VALUES
    (2, 1, 1, '"Second Blog Post (Draft)"'),
    (2, 2, 1, '"second-blog-post"'),
    (2, 3, 1, '"This is a draft post not yet published."'),
    (2, 4, 1, '"<p>Still working on this one...</p>"')

-- Third Blog Post (English + German)
INSERT INTO content_values (content_item_id, field_id, locale_id, value)
VALUES
    (3, 1, 1, '"Third Blog Post"'),
    (3, 1, 2, '"Dritter Blogeintrag"'),
    (3, 2, 1, '"third-blog-post"'),
    (3, 2, 2, '"dritter-blogeintrag"'),
    (3, 3, 1, '"A multilingual blog post example."'),
    (3, 3, 2, '"Ein mehrsprachiges Blogpost-Beispiel."'),
    (3, 4, 1, '"<p>This post has both English and German versions.</p>"'),
    (3, 4, 2, '"<p>Dieser Beitrag ist auf Englisch und Deutsch verfügbar.</p>"'),
    (3, 5, 1, '"2025-10-10T08:00:00Z"');
