-- The development seed uses stable IDs for its cross references. Reset only
-- development content tables so retries after a failed seed stay deterministic.
TRUNCATE TABLE media,
content_categories,
content_tags,
content_entries,
content_nodes,
content_authors,
content_node_templates,
comments RESTART IDENTITY CASCADE;

SELECT
    setval('category_group_seq', 1001, FALSE);

SELECT
    setval('entry_group_seq', 1001, FALSE);

INSERT INTO
    locales (code, name, tsv_dict)
VALUES
    ('fr', 'French', 'french'),
    ('es', 'Spanish', 'spanish')
ON CONFLICT (code) DO NOTHING;

INSERT INTO
    media (alt, filename, path, type, width, height, size, uploaded_by)
VALUES
    ('cover', 'cover.jpg', '/uploads/2025/11', 'image/jpeg', 1280, 853, 63976, 1),
    ('block', 'block.jpg', '/uploads/2025/11', 'image/jpeg', 1280, 867, 57116, 1),
    ('Cat', 'cat.jpg', '/uploads/2025/11', 'image/jpeg', 233, 233, 11414, 1),
    ('Flower', 'flower.jpg', '/uploads/2025/11', 'image/jpeg', 233, 233, 22812, 1);

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
        'published',
        1,
        1
    ),
    (1001, 1, 2, 2, 1, 'first-article', 'Welcome to my blog', 'published', 1, 1),
    (
        nextval('entry_group_seq'),
        2,
        NULL,
        1,
        2,
        'about-us',
        'Über uns',
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
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        1,
        4,
        2,
        3,
        'third-article',
        'Third article',
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
        'published',
        1,
        1
    ),
    (nextval('entry_group_seq'), 2, NULL, 3, NULL, 'faq', 'FAQ', 'published', 1, 1),
    (
        nextval('entry_group_seq'),
        2,
        NULL,
        4,
        NULL,
        'team',
        'Nuestro Equipo',
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
        'published',
        1,
        1
    ),
    (
        nextval('entry_group_seq'),
        1,
        NULL,
        2,
        2,
        'blocks-article',
        'Blocks Article',
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
    content_nodes (entry_id, order_index, text, data, media_id, parent_id)
VALUES
    (
        1,
        1,
        E'Dies ist mein erster Artikel in **Markdown** mit _kursivem Text_, ~~veraltetem Inhalt~~ und `Inline-Code`.\n\n## Erweiterungen\n\n__Unterstrichen__, ==hervorgehoben==, ++eingefügt++, x^2^ und ||ein Spoiler||.\n\n- [x] Markdown aktivieren\n- [ ] Eigenen Inhalt ergänzen\n\n| Funktion | Status |\n| --- | --- |\n| Tabellen | Verfügbar |\n| Aufgaben | Verfügbar |\n\nEin Satz mit einer Fußnote.[^quelle] Ein weiterer Satz mit einer Inline-Fußnote^[Diese Notiz steht direkt im Text.].\n\n[^quelle]: Diese Fußnote wird am Ende des Dokuments ausgegeben.\n\n-# Dieser ergänzende Hinweis wird als Untertext dargestellt.\n\n>>>\nDieses mehrzeilige Zitat kann mehrere Absätze enthalten.\n\nAuch dieser Absatz gehört zum Zitat.\n>>>\n\n> [!TIP]\n> Verwende die Markdown-Hilfe im Editor für weitere Beispiele.\n\n:::notice\nDieser Block verwendet eine Block-Direktive.\n:::\n\nWeitere Informationen gibt es unter https://commonmark.org.\n\n## Bilder\n\nEin eingebettetes Bild: ![Cover](/uploads/2025/11/cover.jpg)\n\nUnd ein weiteres Bild als eigener Block:\n\n![Cover Block](/uploads/2025/11/block.jpg)\n\nHier sind einige externe Bilder:\n\n![Schwarzer Hund](https://picsum.photos/id/237/200/300)\n\n![Landschaft](https://picsum.photos/id/29/200/300)\n\n![Küste](https://picsum.photos/id/19/200/300)',
        NULL,
        NULL,
        NULL
    ),
    (
        2,
        1,
        E'This is my first article in **Markdown** with _italic text_, ~~outdated content~~, and `inline code`.\n\n## Extensions\n\n__Underlined__, ==highlighted==, ++inserted++, x^2^, and ||a spoiler||.\n\n- [x] Enable Markdown\n- [ ] Add your own content\n\n| Feature | Status |\n| --- | --- |\n| Tables | Available |\n| Tasks | Available |\n\nA sentence with a footnote.[^source] Another sentence with an inline footnote^[This note is defined directly in the text.].\n\n[^source]: This footnote is rendered at the end of the document.\n\n-# This additional note is rendered as subtext.\n\n>>>\nThis multiline quote can contain several paragraphs.\n\nThis paragraph is part of the quote as well.\n>>>\n\n> [!TIP]\n> Use the Markdown help in the editor for more examples.\n\n:::notice\nThis block uses a block directive.\n:::\n\nLearn more at https://commonmark.org.\n\n## Images\n\nAn embedded image: ![Cover](/uploads/2025/11/cover.jpg)\n\nAnd another image as its own block:\n\n![Cover Block](/uploads/2025/11/block.jpg)\n\nHere are some external images:\n\n![Black dog](https://picsum.photos/id/237/200/300)\n\n![Landscape](https://picsum.photos/id/29/200/300)\n\n![Coast](https://picsum.photos/id/19/200/300)',
        NULL,
        NULL,
        NULL
    ),
    (3, 1, E'Dies ist die **About** Seite.', NULL, NULL, NULL),
    (
        4,
        1,
        E'Ceci est du **Markdown** pour le deuxième billet.',
        NULL,
        NULL,
        NULL
    ),
    (
        5,
        1,
        E'More content with _italic_ and **bold**.',
        NULL,
        NULL,
        NULL
    ),
    (
        6,
        1,
        E'This page explains our privacy practices.',
        NULL,
        NULL,
        NULL
    ),
    (7, 1, E'Alle rechtlichen Hinweise.', NULL, NULL, NULL),
    (8, 1, E'Nous répondons aux questions fréquentes.', NULL, NULL, NULL),
    (9, 1, E'Detalles sobre los miembros del equipo.', NULL, NULL, NULL),
    (10, 1, E'Information to reach us.', NULL, NULL, NULL),
    (
        11,
        1,
        E'Join us for our first Rust meetup.',
        NULL,
        NULL,
        NULL
    ),
    (12, 1, E'Détails du second meetup.', NULL, NULL, NULL),
    (
        13,
        1,
        E'Details zur jährlichen Konferenz.',
        NULL,
        NULL,
        NULL
    ),
    (
        14,
        1,
        E'Aprende desarrollo web con ejercicios prácticos.',
        NULL,
        NULL,
        NULL
    ),
    (
        15,
        1,
        E'Discussing AI trends and technologies.',
        NULL,
        NULL,
        NULL
    ),
    (16, 1, 'Just a random text', NULL, NULL, NULL),
    (16, 2, NULL, '{"text":"This is the first block of content."}', NULL, NULL),
    (
        16,
        3,
        NULL,
        '{"text":"Here is another paragraph with some bold text."}',
        NULL,
        17
    ),
    (16, 4, NULL, '{"author":"The Cat", "url": "https://example.org"}', 3, 17),
    (16, 5, NULL, '{"text":"Yet another text block with italic text."}', NULL, 17),
    (16, 6, NULL, '{"text":"Last item"}', NULL, NULL);

INSERT INTO
    content_node_templates (name, data)
VALUES
    ('employee', '{"first_name": "", "last_name": "", "position": ""}'),
    ('text', '{"text": ""}'),
    ('author', '{"author": "", "url": ""}');

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
    content_node_media (node_id, media_id, position_index)
VALUES
    (1, 1, 0),
    (1, 2, 1),
    (2, 1, 0),
    (2, 2, 1);

INSERT INTO
    comments (entry_id, parent_id, user_id, author_name, author_email, text, status)
VALUES
    (1, NULL, 1, NULL, NULL, 'Great article! Thanks for sharing this.', 'approved'),
    (
        1,
        NULL,
        NULL,
        'Anna Schmidt',
        'anna@example.com',
        'Very informative post, looking forward to more!',
        'approved'
    ),
    (
        1,
        1,
        NULL,
        'Tom Weber',
        'tom@example.com',
        'I agree, this is really helpful.',
        'approved'
    ),
    (2, NULL, 1, NULL, NULL, 'Excellent content, keep up the good work!', 'approved'),
    (
        2,
        NULL,
        NULL,
        'Maria Garcia',
        'maria@example.com',
        'I have a question about this topic...',
        'pending'
    ),
    (
        4,
        NULL,
        NULL,
        'Pierre Dubois',
        'pierre@example.com',
        'Très bon article, merci!',
        'approved'
    ),
    (10, NULL, 1, NULL, NULL, 'Looking forward to the meetup!', 'approved'),
    (
        10,
        7,
        NULL,
        'Julia Müller',
        'julia@example.com',
        'Me too! What time does it start?',
        'approved'
    ),
    (
        12,
        NULL,
        NULL,
        'Carlos Rodriguez',
        'carlos@example.com',
        'This conference looks amazing.',
        'approved'
    ),
    (
        16,
        NULL,
        NULL,
        'Sarah Johnson',
        'sarah@example.com',
        'Nice use of blocks in this article.',
        'pending'
    );
