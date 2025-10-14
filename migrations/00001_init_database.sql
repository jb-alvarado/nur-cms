CREATE TABLE
    auth_roles (id SERIAL PRIMARY KEY, name VARCHAR(16) NOT NULL UNIQUE);

INSERT INTO
    auth_roles (name)
VALUES
    ('admin'),
    ('author'),
    ('user'),
    ('guest');

CREATE TABLE
    auth_users (
        id SERIAL PRIMARY KEY,
        email VARCHAR(255) NOT NULL,
        username VARCHAR(150) NOT NULL UNIQUE,
        password VARCHAR(255) NOT NULL,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now (),
        last_login TIMESTAMPTZ,
        role_id INTEGER NOT NULL DEFAULT 2,
        CONSTRAINT fk_role FOREIGN KEY (role_id) REFERENCES auth_roles (id) ON UPDATE CASCADE ON DELETE SET DEFAULT
    );

CREATE TABLE
    locales (id SERIAL PRIMARY KEY, code VARCHAR(7) NOT NULL, name VARCHAR(64) NOT NULL);

INSERT INTO
    locales (code, name)
VALUES
    ('de-DE', 'German'),
    ('en-US', 'English (US)'),
    ('fr-FR', 'French'),
    ('es-ES', 'Spanish');

CREATE TABLE
    content_types (
        id SERIAL PRIMARY KEY,
        name TEXT UNIQUE NOT NULL, -- "BlogPost", "Page", "Product"
        slug TEXT UNIQUE NOT NULL,
        description TEXT,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now (),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE TABLE
    fields (
        id SERIAL PRIMARY KEY,
        content_type_id INT REFERENCES content_types (id) ON DELETE CASCADE,
        name TEXT NOT NULL, -- "title", "body", "published_at"
        field_type TEXT NOT NULL, -- "text", "richtext", "number", "boolean", "json"
        required BOOLEAN DEFAULT false,
        order_index INT DEFAULT 0,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now (),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE TABLE
    content_items (
        id SERIAL PRIMARY KEY,
        content_type_id INT REFERENCES content_types (id) ON DELETE CASCADE,
        locale_id INT REFERENCES locales (id) ON DELETE CASCADE,
        slug TEXT NOT NULL,
        status TEXT DEFAULT 'draft', -- draft, published, archived
        created_by INT REFERENCES auth_users (id),
        updated_by INT REFERENCES auth_users (id),
        created_at TIMESTAMPTZ NOT NULL DEFAULT now (),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE TABLE
    content_values (
        id SERIAL PRIMARY KEY,
        content_item_id INT REFERENCES content_items (id) ON DELETE CASCADE,
        field_id INT REFERENCES fields (id) ON DELETE CASCADE,
        value JSONB, -- Text, Number, Bool
        created_at TIMESTAMPTZ NOT NULL DEFAULT now (),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );

CREATE TABLE
    media (
        id SERIAL PRIMARY KEY,
        filename TEXT NOT NULL,
        url TEXT NOT NULL,
        type TEXT,
        uploaded_by INT REFERENCES auth_users (id),
        created_at TIMESTAMPTZ NOT NULL DEFAULT now ()
    );
