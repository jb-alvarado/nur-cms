INSERT INTO
    media (alt, filename, path, type, width, height, size, uploaded_by)
VALUES
    (
        'cover',
        'cover.jpg',
        '/uploads/2025/11',
        'image/jpeg',
        1280,
        853,
        63976,
        1
    ),
    (
        'block',
        'block.jpg',
        '/uploads/2025/11',
        'image/jpeg',
        1280,
        867,
        57116,
        1
    ),
    (
        'Cat',
        'cat.jpg',
        '/uploads/2025/11',
        'image/jpeg',
        233,
        233,
        11414,
        1
    ),
    (
        'Flower',
        'flower.jpg',
        '/uploads/2025/11',
        'image/jpeg',
        233,
        233,
        22812,
        1
    );

INSERT INTO
    media_variants (media_id, width, height, filename)
VALUES
    (1, 320, 213, 'cover-320.avif'),
    (1, 320, 213, 'cover-320.jpg'),
    (1, 320, 213, 'cover-320.webp'),
    (1, 480, 320, 'cover-480.avif'),
    (1, 480, 320, 'cover-480.jpg'),
    (1, 480, 320, 'cover-480.webp'),
    (1, 1024, 682, 'cover-1024.avif'),
    (1, 1024, 682, 'cover-1024.jpg'),
    (1, 1024, 682, 'cover-1024.webp'),
    (2, 320, 217, 'block-320.avif'),
    (2, 320, 217, 'block-320.jpg'),
    (2, 320, 217, 'block-320.webp'),
    (2, 480, 325, 'block-480.avif'),
    (2, 480, 325, 'block-480.jpg'),
    (2, 480, 325, 'block-480.webp'),
    (2, 1024, 694, 'block-1024.avif'),
    (2, 1024, 694, 'block-1024.jpg'),
    (2, 1024, 694, 'block-1024.webp');

INSERT INTO
    content_categories (group_id, locale_id, name, slug, status, media_id)
VALUES
    (
        nextval('category_group_seq'),
        1,
        'Weltweite IT',
        'weltweite-it',
        'published',
        1
    ),
    (1001, 2, 'World Wide IT', 'world-wide-it', 'published', 1),
    (
        nextval('category_group_seq'),
        2,
        'Newest Rust Projects',
        'newest-rust-projects',
        'published',
        2
    ),
    (
        nextval('category_group_seq'),
        2,
        'Open Source Meetings',
        'open-source-meetings',
        'draft',
        NULL
    );

INSERT INTO
    content_tags (name, slug)
VALUES
    ('IT', 'it'),
    ('Rust', 'rust'),
    ('Open Source', 'open-source');

INSERT INTO
    content_entries (
        group_id,
        type_id,
        category_id,
        locale_id,
        media_id,
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
        nextval('entry_group_seq'),
        1,
        1,
        1,
        1,
        'erster-artikel',
        'Willkommen auf meinem Blog',
        'Dies ist mein erster Artikel in Markdown.',
        E'# Hallo Welt\nDies ist mein erster Artikel in **Markdown**.\n\nUnd wir haben einen _zweiten_ Absatz!\n\nUnd ein eingebettetes Bild: ![Cover](/uploads/2025/11/cover.jpg)\n\nAber auch ein Block-Bild:\n\n![Cover Block](/uploads/2025/11/block.jpg)\n\nHier einige HTML-Tags:\n\n<div class="flex justify-center"><div class="grid">\n\n<img src="https://picsum.photos/id/237/200/300" alt="image1" />\n\n<img src="https://picsum.photos/id/29/200/300" alt="image2" />\n\n<img src="https://picsum.photos/id/19/200/300" alt="image3" />\n\n</div></div>\n\nHier haben wir <i>inline</i> HTML.',
        'published',
        1,
        1
    ),
    (
        1001,
        1,
        2,
        2,
        1,
        'first-article',
        'Welcome to my blog',
        'This is my first article in Markdown.',
        E'# Hello World\nThis is my first article in **Markdown**.\n\nAnd we have a _second_ paragraph!\n\nAnd a inline picture: ![Cover](/uploads/2025/11/cover.jpg)\n\nBut also a block picture:\n\n![Cover Block](/uploads/2025/11/block.jpg)\n\nHere some html tags:\n\n<div class="flex justify-center"><div class="grid">\n\n<img src="https://picsum.photos/id/237/200/300" alt="image1" />\n\n<img src="https://picsum.photos/id/29/200/300" alt="image2" />\n\n<img src="https://picsum.photos/id/19/200/300" alt="image3" />\n\n</div></div>\n\nHere we have <i>inline</i> html.',
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        2,
        NULL,
        1,
        2,
        'about-us',
        'Über uns',
        NULL,
        E'Dies ist die **About** Seite.',
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        1,
        3,
        3,
        2,
        'second-article',
        'Un autre billet',
        'This is the second article.',
        E'# Deuxième billet\nCeci est du **Markdown** pour le deuxième billet.',
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        1,
        4,
        2,
        NULL,
        'third-article',
        'Otro article',
        'Third article description.',
        E'# Tercer Post\nMás contenido con _italic_ y **bold**.',
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        2,
        NULL,
        2,
        NULL,
        'privacy-policy',
        'Privacy Policy',
        'Information about privacy.',
        E'# Privacy Policy\nThis page explains our privacy practices.',
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        2,
        NULL,
        1,
        NULL,
        'terms-of-service',
        'Nutzungsbedingungen',
        'Terms and conditions.',
        E'# Nutzungsbedingungen\nAlle rechtlichen Hinweise.',
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        2,
        NULL,
        3,
        NULL,
        'faq',
        'FAQ',
        'Common questions answered.',
        E'# FAQ\nNous répondons aux questions fréquentes.',
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        2,
        NULL,
        4,
        NULL,
        'team',
        'Nuestro Equipo',
        'Meet the team.',
        E'# Equipo\nDetalles sobre los miembros del equipo.',
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        2,
        NULL,
        2,
        NULL,
        'contact',
        'Contact Us',
        'How to contact us.',
        E'# Contact\nInformation to reach us.',
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        3,
        NULL,
        2,
        NULL,
        'rust-meetup-1',
        'Rust Meetup #1',
        'First Rust meetup.',
        E'# Rust Meetup #1\nJoin us for our first Rust meetup.',
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        3,
        NULL,
        3,
        NULL,
        'rust-meetup-2',
        'Rencontre Rust #2',
        'Second Rust meetup.',
        E'# Rust Meetup #2\nDétails du second meetup.',
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        3,
        NULL,
        1,
        NULL,
        'opensource-conference',
        'Open Source Konferenz',
        'Annual conference.',
        E'# Open Source Konferenz\nDetails zur jährlichen Konferenz.',
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        3,
        NULL,
        4,
        NULL,
        'web-dev-workshop',
        'Taller de Desarrollo Web',
        'Hands-on workshop.',
        E'# Taller de Desarrollo Web\nAprende desarrollo web con ejercicios prácticos.',
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        3,
        NULL,
        2,
        NULL,
        'ai-seminar',
        'AI Seminar',
        'Seminar about AI.',
        E'# AI Seminar\nDiscussing AI trends and technologies.',
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        1,
        NULL,
        2,
        NULL,
        'blocks-article',
        'Blocks Article',
        'An article with blocks.',
        '',
        'published',
        1,
        1
    );

INSERT INTO
    content_authors (first_name, last_name, slug, bio, media_id)
VALUES
    (
        'Max',
        'Mustermann',
        'max-mustermann',
        'Max likes hiking, music and church planting.',
        3
    ),
    (
        'Lisa',
        'Musterfrau',
        'lisa-musterfrau',
        'Lisa likes praying, walking and cats.',
        4
    );

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
    content_media (entry_id, media_id, ast_line)
VALUES
    (1, 1, 6),
    (1, 2, 10);
