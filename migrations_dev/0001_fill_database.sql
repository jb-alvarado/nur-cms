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
        group_id,
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
        10001,
        1,
        1,
        'erster-artikel',
        'Willkommen auf meinem Blog',
        'Dies ist mein erster Artikel in Markdown.',
        E'# Hallo Welt\nDies ist mein erster Artikel in **Markdown**.\n\nUnd wir haben einen _zweiten_ Absatz!\n\nUnd ein eingebettetes Bild: ![Cover](/uploads/2025/10/cover.jpg)\n\nAber auch ein Block-Bild:\n\n![Cover Block](/uploads/2025/10/block.jpg)\n\nHier einige HTML-Tags:\n\n<div class="flex justify-center"><div class="grid">\n\n<img src="https://example.org/image1.jpg" alt="image1" />\n\n<img src="https://example.org/image2.jpg" alt="image2" />\n\n<img src="https://example.org/image3.jpg" alt="image3" />\n\n</div></div>\n\nHier haben wir <i>inline</i> HTML.',
        'published',
        1,
        1
    ),
    (
        2,
        10001,
        1,
        2,
        'first-article',
        'Welcome to my blog',
        'This is my first article in Markdown.',
        E'# Hello World\nThis is my first article in **Markdown**.\n\nAnd we have a _second_ paragraph!\n\nAnd a inline picture: ![Cover](/uploads/2025/10/cover.jpg)\n\nBut also a block picture:\n\n![Cover Block](/uploads/2025/10/block.jpg)\n\nHere some html tags:\n\n<div class="flex justify-center"><div class="grid">\n\n<img src="https://example.org/image1.jpg" alt="image1" />\n\n<img src="https://example.org/image2.jpg" alt="image2" />\n\n<img src="https://example.org/image3.jpg" alt="image3" />\n\n</div></div>\n\nHere we have <i>inline</i> html.',
        'published',
        1,
        1
    ),
    (
        3,
        10002,
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
        4,
        10003,
        1,
        3,
        'second-article',
        'Un autre billet',
        'This is the second article.',
        E'# Deuxième billet\nCeci est du **Markdown** pour le deuxième billet.',
        'published',
        1,
        1
    ),
    (
        5,
        10004,
        1,
        4,
        'third-article',
        'Otro article',
        'Third article description.',
        E'# Tercer Post\nMás contenido con _italic_ y **bold**.',
        'published',
        1,
        1
    ),
    (
        6,
        10005,
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
        7,
        10006,
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
        8,
        10007,
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
        9,
        10008,
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
        10,
        10009,
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
        11,
        10010,
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
        12,
        10011,
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
        13,
        10012,
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
        14,
        10013,
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
        15,
        10014,
        3,
        2,
        'ai-seminar',
        'AI Seminar',
        'Seminar about AI.',
        E'# AI Seminar\nDiscussing AI trends and technologies.',
        'published',
        1,
        1
    ),
    (
        16,
        10015,
        1,
        2,
        'blocks-article',
        'Blocks Article',
        'An article with blocks.',
        '',
        'published',
        1,
        1
    );

INSERT INTO
    content_authors (id, first_name, last_name, slug, bio)
VALUES
    (1, 'Max', 'Mustermann', 'max-mustermann', 'Max likes hiking, music and church planting.'),
    (2, 'Lisa', 'Musterfrau', 'lisa-musterfrau', 'Lisa likes praying, walking and cats.');

INSERT INTO
    content_entry_authors (entry_id, author_id)
VALUES
    (1, 1),
    (2, 1),
    (4, 2);

INSERT INTO
    content_meta (entry_id, start_time, end_time)
VALUES
    (10, '2025-11-03T08:00:00Z', '2025-11-05T19:00:00Z'),
    (11, '2025-12-11T08:30:00Z', '2025-12-11T18:30:00Z'),
    (12, '2025-11-18T09:00:00Z', '2025-11-21T17:00:00Z'),
    (13, '2026-01-13T08:30:00Z', '2026-01-15T13:00:00Z'),
    (14, '2026-02-10T10:15:00Z', '2026-02-12T12:15:00Z');

INSERT INTO
    content_blocks (entry_id, type, order_index, data)
VALUES
    (
        16,
        'paragraph',
        0,
        '{"children":[{"text":"This is the first block of content.","type":"text"}]}'
    ),
    (
        16,
        'paragraph',
        1,
        '{"children":[{"text":"Here is another paragraph with some bold text.","type":"text","bold":true}]}'
    ),
    (
        16,
        'image',
        2,
        '{"alt":"Cover Image","filename":"cover.jpg","path":"/uploads/2025/10/cover.jpg","type":"image"}'
    ),
    (
        16,
        'paragraph',
        3,
        '{"children":[{"text":"Yet another text block with italic text.","type":"text","italic":true}]}'
    ),
    (
        16,
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
    (
        'Cover',
        'cover.jpg',
        '/uploads/2025/10/cover.jpg',
        'image',
        1
    ),
    (
        'Block',
        'block.jpg',
        '/uploads/2025/10/block.jpg',
        'image',
        1
    );

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
