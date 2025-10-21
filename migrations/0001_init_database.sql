CREATE TABLE auth_roles (
    id SERIAL PRIMARY KEY,
    name VARCHAR(16) NOT NULL UNIQUE DEFAULT 'guest'
);

INSERT INTO
    auth_roles (name)
VALUES
    ('admin'),
    ('author'),
    ('user'),
    ('guest');

CREATE TABLE auth_users (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) NOT NULL UNIQUE,
    username VARCHAR(150) NOT NULL UNIQUE,
    first_name VARCHAR(150) NOT NULL,
    last_name VARCHAR(150) NOT NULL,
    password VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_login TIMESTAMPTZ,
    role_id INTEGER NOT NULL DEFAULT 2,
    CONSTRAINT fk_role FOREIGN KEY (role_id) REFERENCES auth_roles (id) ON UPDATE CASCADE ON DELETE SET DEFAULT
);

CREATE TABLE locales (
    id SERIAL PRIMARY KEY,
    code VARCHAR(7) NOT NULL,
    name VARCHAR(64) NOT NULL,
    tsv_dict VARCHAR(24) NOT NULL DEFAULT 'simple'
);

INSERT INTO
    locales (code, name, tsv_dict)
VALUES
    ('de-DE', 'German', 'german'),
    ('en-US', 'English (US)', 'english'),
    ('fr-FR', 'French', 'french'),
    ('es-ES', 'Spanish', 'spanish');

CREATE TABLE content_types (
    id SERIAL PRIMARY KEY,
    name VARCHAR(12) UNIQUE NOT NULL, -- "BlogPost", "Page", "Event"
    slug VARCHAR(32) UNIQUE NOT NULL
);

CREATE TABLE content_categories (
    id SERIAL PRIMARY KEY,
    locale_id INT NOT NULL REFERENCES locales (id) ON DELETE CASCADE,
    name VARCHAR(160) NOT NULL,
    slug VARCHAR(160) NOT NULL,
    UNIQUE (slug, locale_id)
);

CREATE TABLE content_tags (
    id SERIAL PRIMARY KEY,
    locale_id INT NOT NULL REFERENCES locales (id) ON DELETE CASCADE,
    name VARCHAR(160) NOT NULL,
    slug VARCHAR(160) NOT NULL,
    UNIQUE (slug, locale_id)
);

CREATE TABLE content_entries (
    id SERIAL PRIMARY KEY,
    type_id INT NOT NULL REFERENCES content_types (id) ON DELETE CASCADE,
    locale_id INT NOT NULL REFERENCES locales (id) ON DELETE CASCADE,
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    text TEXT,
    text_vector TSVECTOR, -- for full text search, fill on insert
    status VARCHAR(16) CHECK (status IN ('draft', 'published', 'archived')) DEFAULT 'draft', -- draft, published, archived
    created_by INT REFERENCES auth_users (id),
    updated_by INT REFERENCES auth_users (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (slug, locale_id, type_id)
);

CREATE TABLE content_attributes (
    id SERIAL PRIMARY KEY,
    entry_id INT NOT NULL REFERENCES content_entries (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    value JSONB NOT NULL,
    UNIQUE (entry_id, name)
);

CREATE TABLE content_blocks (
    id SERIAL PRIMARY KEY,
    entry_id INT REFERENCES content_entries (id) ON DELETE CASCADE,
    type VARCHAR(64) NOT NULL,
    order_index INT NOT NULL DEFAULT 0,
    data JSONB NOT NULL
);

CREATE TABLE content_entry_categories (
    entry_id INT REFERENCES content_entries (id) ON DELETE CASCADE,
    category_id INT REFERENCES content_categories (id) ON DELETE CASCADE,
    PRIMARY KEY (entry_id, category_id)
);

CREATE TABLE content_entry_tags (
    entry_id INT REFERENCES content_entries (id) ON DELETE CASCADE,
    tag_id INT REFERENCES content_tags (id) ON DELETE CASCADE,
    PRIMARY KEY (entry_id, tag_id)
);

CREATE TABLE media (
    id SERIAL PRIMARY KEY,
    alt TEXT,
    filename TEXT NOT NULL,
    path TEXT NOT NULL,
    type TEXT,
    uploaded_by INT REFERENCES auth_users (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE media_variants (
    id SERIAL PRIMARY KEY,
    media_id INT REFERENCES media (id) ON DELETE CASCADE,
    resolution INT NOT NULL,
    format TEXT NOT NULL DEFAULT 'jpg',
    filename TEXT NOT NULL
);

CREATE TABLE content_media (
    id SERIAL PRIMARY KEY,
    entry_id INT NOT NULL REFERENCES content_entries (id) ON DELETE CASCADE,
    media_id INT NOT NULL REFERENCES media (id) ON DELETE CASCADE,
    ast_line INT NOT NULL DEFAULT 0,
    start_offset INT,
    end_offset INT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (entry_id, media_id, ast_line)
);

CREATE OR REPLACE FUNCTION content_text_vector_update () RETURNS trigger AS $$
DECLARE
    dict TEXT;
BEGIN
    SELECT l.tsv_dict INTO dict FROM locales l WHERE l.id = NEW.locale_id;
    IF dict IS NULL THEN
        dict := 'simple';
    END IF;

    NEW.text_vector := to_tsvector(dict::regconfig,
        COALESCE(NEW.title, '') || ' ' || COALESCE(NEW.text, ''));

    RETURN NEW;
END
$$ LANGUAGE plpgsql;

CREATE TRIGGER content_text_vector_trigger BEFORE INSERT
OR
UPDATE ON content_entries FOR EACH ROW
EXECUTE FUNCTION content_text_vector_update ();
