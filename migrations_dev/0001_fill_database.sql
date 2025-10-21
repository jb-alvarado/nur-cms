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
    content_entries (
        id,
        type_id,
        locale_id,
        slug,
        title,
        description,
        text,
        status,
        created_by,
        updated_by
    )
VALUES
    (
        1,
        1,
        2,
        'first-blog-post',
        'Welcome to my blog',
        'This is my first blog post in Markdown.',
        E'# Hello World\nThis is my first blog post in **Markdown**.\n\nAnd we have a _second_ paragraph!\n\nAnd a inline picture: ![Cover](/uploads/2025/10/cover.jpg)\n\nBut also a block picture:\n\n![Cover Block](/uploads/2025/10/block.jpg)\n\nHere some html tags:\n\n<div class="flex justify-center"><div class="grid">\n\n<img src="https://example.org/image1.jpg" alt="image1" />\n\n<img src="https://example.org/image2.jpg" alt="image2" />\n\n<img src="https://example.org/image3.jpg" alt="image3" />\n\n</div></div>\n\nHere we have <i>inline</i> html.',
        'published',
        1,
        1
    ),
    (
        2,
        2,
        1,
        'about-us',
        'Über uns',
        NULL,
        E'Dies ist die **About** Seite.',
        'published',
        1,
        1
    ),
    (
        3,
        1,
        3,
        'second-blog-post',
        'Un autre billet',
        'This is the second blog post.',
        E'# Deuxième billet\nCeci est du **Markdown** pour le deuxième billet.',
        'published',
        1,
        1
    ),
    (
        4,
        1,
        4,
        'third-blog-post',
        'Otro Blog Post',
        'Third blog post description.',
        E'# Tercer Post\nMás contenido con _italic_ y **bold**.',
        'published',
        1,
        1
    ),
    (
        5,
        2,
        2,
        'privacy-policy',
        'Privacy Policy',
        'Information about privacy.',
        E'# Privacy Policy\nThis page explains our privacy practices.',
        'published',
        1,
        1
    ),
    (
        6,
        2,
        1,
        'terms-of-service',
        'Nutzungsbedingungen',
        'Terms and conditions.',
        E'# Nutzungsbedingungen\nAlle rechtlichen Hinweise.',
        'published',
        1,
        1
    ),
    (
        7,
        2,
        3,
        'faq',
        'FAQ',
        'Common questions answered.',
        E'# FAQ\nNous répondons aux questions fréquentes.',
        'published',
        1,
        1
    ),
    (
        8,
        2,
        4,
        'team',
        'Nuestro Equipo',
        'Meet the team.',
        E'# Equipo\nDetalles sobre los miembros del equipo.',
        'published',
        1,
        1
    ),
    (
        9,
        2,
        2,
        'contact',
        'Contact Us',
        'How to contact us.',
        E'# Contact\nInformation to reach us.',
        'published',
        1,
        1
    ),
    (
        10,
        3,
        2,
        'rust-meetup-1',
        'Rust Meetup #1',
        'First Rust meetup.',
        E'# Rust Meetup #1\nJoin us for our first Rust meetup.',
        'published',
        1,
        1
    ),
    (
        11,
        3,
        3,
        'rust-meetup-2',
        'Rencontre Rust #2',
        'Second Rust meetup.',
        E'# Rust Meetup #2\nDétails du second meetup.',
        'published',
        1,
        1
    ),
    (
        12,
        3,
        1,
        'opensource-conference',
        'Open Source Konferenz',
        'Annual conference.',
        E'# Open Source Konferenz\nDetails zur jährlichen Konferenz.',
        'published',
        1,
        1
    ),
    (
        13,
        3,
        4,
        'web-dev-workshop',
        'Taller de Desarrollo Web',
        'Hands-on workshop.',
        E'# Taller de Desarrollo Web\nAprende desarrollo web con ejercicios prácticos.',
        'published',
        1,
        1
    ),
    (
        14,
        3,
        2,
        'ai-seminar',
        'AI Seminar',
        'Seminar about AI.',
        E'# AI Seminar\nDiscussing AI trends and technologies.',
        'published',
        1,
        1
    );

INSERT INTO
    content_blocks (entry_id, type, order_index, data)
VALUES
    (
        1,
        'paragraph',
        0,
        '{"children":[{"text":"This is the first block of content.","type":"text"}]}'
    ),
    (
        1,
        'paragraph',
        1,
        '{"children":[{"text":"Here is another paragraph with some bold text.","type":"text","bold":true}]}'
    ),
    (
        1,
        'image',
        2,
        '{"alt":"Cover Image","filename":"cover.jpg","path":"/uploads/2025/10/cover.jpg","type":"image"}'
    ),
    (
        1,
        'paragraph',
        3,
        '{"children":[{"text":"Yet another text block with italic text.","type":"text","italic":true}]}'
    ),
    (
        1,
        'list',
        4,
        '{"items":[{"text":"First list item"},{"text":"Second list item"}],"type":"unordered"}'
    );

INSERT INTO
    content_entry_categories (entry_id, category_id)
VALUES
    (1, 1),
    (3, 1),
    (4, 2),
    (5, 3),
    (6, 3),
    (10, 2),
    (11, 2);

INSERT INTO
    content_entry_tags (entry_id, tag_id)
VALUES
    (1, 1),
    (1, 2),
    (3, 1),
    (3, 2),
    (4, 2),
    (10, 2),
    (12, 3);

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
