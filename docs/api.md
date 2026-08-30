# Backend API

This reference documents the HTTP API of the current `nur-cms` backend. The
examples use `http://127.0.0.1:8777` as the base URL.

## Basics

### Prefixes and formats

- Authentication: `/auth`
- REST API: `/api`
- Server-Sent Events: `/sse`
- Uploaded files: `/uploads`
- JSON requests require `Content-Type: application/json`.
- Timestamps use RFC 3339, for example `2026-08-24T12:30:00Z`.
- Collection routes mounted through an Axum nested router use the mount path
  without a trailing slash, for example `/api/comments`.

The backend serves `/uploads/*` itself in debug mode and when the
`--serve-static` option is enabled. In other production configurations, the
upstream web server must expose the configured storage directory under this
URL prefix.

### Authentication

Protected endpoints expect an access token:

```http
Authorization: Bearer <access-token>
```

A request without an `Authorization` header is treated as `guest`. A malformed
header or an invalid or expired access token results in `401 Unauthorized`,
even if the endpoint itself is public.

The built-in roles are:

- `admin`
- `author`
- `user`
- `guest`

For built-in core endpoints, custom roles can currently read only their own
user profile and request an SSE UUID. Plugins may explicitly grant routes to
custom role names in their manifests.

### Standard responses

Unless documented otherwise, collection endpoints use this response format:

```json
{
    "count": 123,
    "next": "/api/content/entries?limit=20&offset=20",
    "previous": null,
    "results": []
}
```

`next` and `previous` are `null` when the corresponding page does not exist.
Only fields requested through `fields` are returned. Empty optional fields may
be omitted from the JSON response.

Successful inserts return the numeric ID as a JSON value:

```json
42
```

Successful updates and deletes usually return `200 OK` with an empty body.
`POST /auth/logout` returns `204 No Content`.

### Errors

API errors usually have this format:

```json
{
    "error": "Error message"
}
```

For some domain errors, the authentication endpoints use this format instead:

```json
{
    "detail": "Error message"
}
```

Relevant status codes:

| Status | Meaning                                                      |
| ------ | ------------------------------------------------------------ |
| `400`  | Invalid query parameters, JSON data, or upload metadata      |
| `401`  | Invalid or expired access token                              |
| `403`  | Login failed or the current role is not permitted            |
| `404`  | Resource not found; the response body is empty               |
| `409`  | Conflict, such as a duplicate slug or detected spam          |
| `422`  | Unsupported field or unprocessable resource                  |
| `429`  | Rate limit exceeded                                          |
| `500`  | Internal error; details are available only in the server log |
| `503`  | A dependency such as email or 2FA is unavailable             |

### Pagination, field selection, and ordering

Most GET collections support:

| Parameter  | Default            | Description                                                  |
| ---------- | ------------------ | ------------------------------------------------------------ |
| `limit`    | `50`               | Number of results; allowed range is `1` to `200`             |
| `offset`   | `0`                | Starting position; allowed range is `0` to `1000000`         |
| `fields`   | all default fields | Comma-separated field selection                              |
| `ordering` | `created_at DESC`  | Comma-separated ordering, for example `name ASC,-created_at` |
| `id`       | –                  | Numeric ID filter where supported                            |
| `search`   | –                  | Resource-specific search                                     |

Ordering fields must belong to the requested resource. Unsupported ordering
fields are ignored. If every value in `fields` is invalid, the backend falls
back to the default fields.

Available fields:

| Resource       | `fields` values                                                                                  |
| -------------- | ------------------------------------------------------------------------------------------------ |
| Roles          | `id,name`                                                                                        |
| Users          | `id,email,username,first_name,last_name,created_at,updated_at,last_login,role`                   |
| Locales        | `id,code,name,tsv_dict`                                                                          |
| Content types  | `id,name,slug,order_index,use_meta`                                                              |
| Categories     | `id,group_id,locale_id,name,slug,status,media_id,media,group_members`                            |
| Tags           | `id,name,slug`                                                                                   |
| Authors        | `id,first_name,last_name,slug,bio,media_id,media`                                                |
| Entries        | See “Content entries”                                                                            |
| Comments       | `id,entry_id,parent_id,user_id,author_name,author_email,text,status,created_at,updated_at,entry` |
| Media          | `id,alt,filename,path,type,width,height,size,uploaded_by,created_at,media_variants`              |
| Node templates | `id,name,data`                                                                                   |
| Mail targets   | `id,name,subject,recipients,allow_html`                                                          |

## Authentication and token rotation

### `POST /auth/login`

Public. Verifies a username and password. The request body is limited to 16
KiB.

```json
{
    "username": "admin",
    "password": "secret"
}
```

When SMTP and 2FA are configured and 2FA has not been disabled through the
startup option, the backend sends a seven-digit code by email:

```json
{
    "detail": "Verification code sended to email!"
}
```

The code is valid for five minutes. Without active email-based 2FA, the
endpoint immediately returns an access and refresh token:

```json
{
    "access": "<jwt>",
    "refresh": "<jwt>"
}
```

### `POST /auth/verify`

Public. Completes a previously initiated 2FA login.

```json
{
    "username": "admin",
    "code": "1234567"
}
```

On success, the response is the same token pair returned by `/auth/login`.
After five failed attempts, the stored code is discarded.

### `POST /auth/refresh`

Public. Exchanges a valid refresh token for a new token pair and atomically
rotates the refresh token in the database.

```json
{
    "refresh": "<refresh-token>"
}
```

The backend rejects refresh tokens that have already been used, revoked,
expired, or are otherwise invalid. A successful response contains both
`access` and `refresh`; the client must replace both stored tokens.

### `POST /auth/logout`

Public. Revokes the current token family using the refresh token. The endpoint
is intentionally idempotent and also returns `204 No Content` for an already
invalid token.

```json
{
    "refresh": "<refresh-token>"
}
```

## Content entries

### Routes

| Method   | Path                                 | Access            | Description                                  |
| -------- | ------------------------------------ | ----------------- | -------------------------------------------- |
| `GET`    | `/api/content/entries`               | Public            | Paginated entry list                         |
| `GET`    | `/api/content/entries/{type}/{slug}` | Public            | Single entry by content-type and entry slug  |
| `GET`    | `/api/content/entries/facets`        | Public            | Available filter facets with result counts   |
| `POST`   | `/api/content/entries`               | `admin`, `author` | Create an entry including nodes and metadata |
| `PUT`    | `/api/content/entries/{id}`          | `admin`, `author` | Update an entry, its nodes, and metadata     |
| `DELETE` | `/api/content/entries/{id}`          | `admin`, `author` | Delete an entry                              |

GET requests without the `admin` or `author` role return only entries with
`status=published`. Administrators and authors can also retrieve drafts and
archived entries and filter by `status`.

### List filters

In addition to pagination, `fields`, and `ordering`, the entry list supports:

| Parameter         | Description                                                              |
| ----------------- | ------------------------------------------------------------------------ |
| `type`            | Content-type slug                                                        |
| `type_id`         | Content-type ID                                                          |
| `exclude_types`   | Comma-separated content-type IDs to exclude                              |
| `locale`          | Locale code                                                              |
| `locale_id`       | Locale ID                                                                |
| `category`        | Category slug                                                            |
| `tag`             | Tag slug                                                                 |
| `author`          | Author slug                                                              |
| `slug`            | Exact entry slug                                                         |
| `status`          | `draft`, `published`, or `archived`; always overridden for public access |
| `id`              | Entry ID                                                                 |
| `group_id`        | Translation group ID                                                     |
| `grouped`         | Return results grouped by `group_id` when `true`                         |
| `search`          | Search titles, authors, and localized node full text                     |
| `data`            | Node-data containment filter, e.g. `hidden:true` or `featured:true,priority:2`; repeat it for independent AND filters, such as `data=hidden:true&data=mainpage:true` |
| `created_after`   | Inclusive lower bound for `created_at`                                   |
| `created_before`  | Exclusive upper bound for `created_at`                                   |
| `start_time`      | Lower time bound for content metadata                                    |
| `end_time`        | Upper time bound for content metadata                                    |
| `output_type`     | `ast`, `html`, or `markdown`; only `admin`/`author` may override it      |
| `character_limit` | AST text limit from `1` to `100000` characters                           |
| `node`            | Exact node names or `@text`, comma-separated; `node_name` is an alias    |
| `node_limit`      | Return at most `1` to `1000` nodes per entry; `blocks_limit` is an alias |
| `blocks_random`   | Select nodes randomly instead of by their order                          |

Within `node`, the reserved value `@text` selects every node whose stored
`text` value is not null, regardless of its name. It can be combined with
stored names, for example `node=block,@text`. The selector only
affects filtering; it does not replace a null `name` in the response.

Each `data` occurrence matches one node and combines its comma-separated
fields with AND. Repeated `data` parameters are also combined with AND, but
may match different nodes of the same entry. Requests accept at most eight
`data` parameters, 16 fields per parameter, and 2048 bytes per parameter.
Values use JSON scalar or structured syntax; unquoted values are strings.

Entry fields:

```text
id,group_id,category_id,locale_id,media_id,slug,status,type,tags,meta,
title,created_at,updated_at,group_members,media,comment_count
```

Nested fields use dot notation:

```text
author.id,author.first_name,author.last_name,author.slug,author.bio,
author.media_id,author.media

category.id,category.group_id,category.locale_id,category.name,
category.slug,category.status,category.media_id,category.media,
category.group_members

node.id,node.entry_id,node.order_index,node.blocks,node.name,node.text,
node.ast,node.html,node.data,node.template_id,node.media_id,node.parent_id,node.media,node.embeds
```

`node.text`, `node.ast`, and `node.html` internally select the same stored
Markdown text. The actual response field depends on `output_type`:

- `markdown`: `text`
- `html`: `html`
- `ast`: `ast`

Example:

```http
GET /api/content/entries?type=article&locale=en&status=published&fields=id,title,slug,node.html&ordering=-created_at&limit=20
```

### Single entry

```http
GET /api/content/entries/article/my-article?fields=id,title,slug,node.ast
```

The response is an entry object directly, not wrapped in `results`. When no
matching published entry exists, the endpoint returns `404` with an empty
body.

### Facets

```http
GET /api/content/entries/facets?type=article&locale=en&category=ideas&tag=vue&author=max&search=axum
```

All parameters are optional. Facets consider published entries only. Each
facet ignores its own active filter while applying all other filters. This
keeps alternative values within the same facet selectable.

```json
{
    "categories": [{ "name": "Ideas", "slug": "ideas", "count": 9 }],
    "tags": [{ "name": "Vue", "slug": "vue", "count": 12 }],
    "authors": [
        {
            "first_name": "Max",
            "last_name": null,
            "slug": "max",
            "count": 4
        }
    ],
    "locales": [{ "code": "en", "name": "English", "count": 15 }]
}
```

### Writing entries

A typical insert request:

```json
{
    "title": "Example",
    "slug": "example",
    "locale_id": 1,
    "category_id": 1,
    "media_id": null,
    "status": "draft",
    "type_id": 1,
    "nodes": [
        {
            "name": "intro",
            "text": "Markdown content",
            "data": null,
            "media_id": null
        },
        {
            "blocks": [
                { "name": "block", "text": "Parent block" },
                { "name": "item", "text": "Child block" }
            ]
        }
    ],
    "meta": {
        "start_time": "2026-08-24T12:00:00Z",
        "end_time": null
    }
}
```

The server sets `created_by`, `updated_by`, `created_at`, `updated_at`, node
`entry_id`, and node `order_index`. When a slug conflicts within the same
content type and locale, the backend appends a random suffix.

Scalar entry fields are optional during updates. When `nodes` is present, the
complete node list is synchronized and omitted existing nodes are deleted.
Existing nodes must include their `id`. Metadata is inserted or updated using
the entry ID.

## Content types, categories, tags, and authors

### Content types

| Method   | Path                      | Access            |
| -------- | ------------------------- | ----------------- |
| `GET`    | `/api/content/types`      | Public            |
| `POST`   | `/api/content/types`      | `admin`           |
| `PUT`    | `/api/content/types/{id}` | `admin`, `author` |
| `DELETE` | `/api/content/types/{id}` | `admin`           |

Body fields: `name`, `slug`, `order_index`, `use_meta`.

```json
{
    "name": "Note",
    "slug": "note",
    "order_index": 4,
    "use_meta": false
}
```

### Categories

| Method   | Path                           | Access                                            |
| -------- | ------------------------------ | ------------------------------------------------- |
| `GET`    | `/api/content/categories`      | Public; only `published` without `admin`/`author` |
| `POST`   | `/api/content/categories`      | `admin`, `author`                                 |
| `PUT`    | `/api/content/categories/{id}` | `admin`, `author`                                 |
| `DELETE` | `/api/content/categories/{id}` | `admin`, `author`                                 |

Additional GET filters: `id`, `locale_id`, `locale`, `slug`, `status`,
`group_id`, `grouped`, and `search`. Body fields: `group_id`, `locale_id`,
`name`, `slug`, `status`, and `media_id`.

```json
{
    "locale_id": 1,
    "name": "Ideas",
    "slug": "ideas",
    "status": "published",
    "media_id": null
}
```

### Tags

| Method | Path                     | Access            |
| ------ | ------------------------ | ----------------- |
| `GET`  | `/api/content/tags`      | Public            |
| `POST` | `/api/content/tags`      | `admin`, `author` |
| `PUT`  | `/api/content/tags/{id}` | `admin`, `author` |

Body fields: `name`, `slug`.

### Authors

| Method   | Path                        | Access            |
| -------- | --------------------------- | ----------------- |
| `GET`    | `/api/content/authors`      | Public            |
| `POST`   | `/api/content/authors`      | `admin`, `author` |
| `PUT`    | `/api/content/authors/{id}` | `admin`, `author` |
| `DELETE` | `/api/content/authors/{id}` | `admin`, `author` |

Additional GET filters: `id`, `search`, `created_after`, and `created_before`.
Body fields: `first_name`, `last_name`, `slug`, `bio`, and `media_id`.
`first_name` and `slug` are required; `last_name` may be `null`.

```json
{
    "first_name": "Max",
    "last_name": null,
    "slug": "max",
    "bio": null,
    "media_id": null
}
```

### Entry associations

| Method   | Path                                                 | Access            | Body                           |
| -------- | ---------------------------------------------------- | ----------------- | ------------------------------ |
| `POST`   | `/api/content/entries/tag`                           | `admin`, `author` | `{"entry_id":1,"tag_id":2}`    |
| `DELETE` | `/api/content/entries/{entry_id}/tag/{tag_id}`       | `admin`, `author` | –                              |
| `POST`   | `/api/content/entries/author`                        | `admin`, `author` | `{"entry_id":1,"author_id":2}` |
| `DELETE` | `/api/content/entries/{entry_id}/author/{author_id}` | `admin`, `author` | –                              |

When the last association of a tag is deleted, the backend currently also
deletes the now-unused tag record.

## Locales and full-text search languages

| Method   | Path                | Access            | Description                                          |
| -------- | ------------------- | ----------------- | ---------------------------------------------------- |
| `GET`    | `/api/locales`      | Public            | List locales                                         |
| `POST`   | `/api/locales`      | `admin`           | Create a locale                                      |
| `PUT`    | `/api/locales/{id}` | `admin`           | Update a locale                                      |
| `DELETE` | `/api/locales/{id}` | `admin`           | Delete a locale                                      |
| `GET`    | `/api/ts-language`  | `admin`, `author` | List available PostgreSQL text-search configurations |

Locale body:

```json
{
    "code": "en",
    "name": "English",
    "tsv_dict": "english"
}
```

`tsv_dict` must name a text-search configuration available in PostgreSQL.

## Comments

| Method   | Path                 | Access                                                    |
| -------- | -------------------- | --------------------------------------------------------- |
| `GET`    | `/api/comments`      | Restricted public view; full access for `admin`, `author` |
| `POST`   | `/api/comments`      | Public                                                    |
| `PUT`    | `/api/comments/{id}` | `admin`, `author`                                         |
| `DELETE` | `/api/comments/{id}` | `admin`, `author`                                         |

Administrators and authors can filter by `id`, `entry_id`, `slug`, `status`,
and `search`. Public GET requests must include `slug=<entry-slug>`. They return
only approved comments and only the fields `id`, `author_name`, `text`,
`created_at`, and `parent_id`.

Anonymous comment:

```json
{
    "entry_id": 42,
    "parent_id": null,
    "author_name": "Ada",
    "author_email": "ada@example.org",
    "text": "My comment"
}
```

Guests must provide `author_name` and `author_email`. The backend validates the
email address and checks the text for spam; new comments receive the `pending`
status. Authenticated users do not need to provide name or email fields. Text
must not be empty and is limited to 20,000 characters; names are limited to
160 characters.

Update fields: `entry_id`, `parent_id`, `user_id`, `author_name`,
`author_email`, `text`, `status`, and `updated_at`. The backend sets
`updated_at` itself.

## Media and uploads

### Media management

| Method   | Path              | Access                    |
| -------- | ----------------- | ------------------------- |
| `GET`    | `/api/media`      | `admin`, `author`, `user` |
| `PUT`    | `/api/media/{id}` | `admin`, `author`         |
| `DELETE` | `/api/media/{id}` | `admin`, `author`         |

Additional GET filters:

- `id`: media ID
- `search`: filename substring
- `media_type`: comma-separated top-level MIME types, for example
  `image,video`

An update accepts only `alt` and `filename`:

```json
{
    "alt": "Description of the image",
    "filename": "new-name.webp"
}
```

The file extension cannot be changed. Deleting a media record removes the
original file, its variants, and the database record.

### Upload status

```http
GET /api/upload?file_name=image.jpg&size=4746903&batch_id=<unique-id>
```

Access: `admin`, `author`.

```json
{
    "received_ranges": [[0, 1048576]],
    "complete": false
}
```

Ranges are byte intervals in the form `[start, end]`. The client can use them
to resume an interrupted upload.

### Upload chunk

```http
POST /api/upload
Content-Type: multipart/form-data
```

Access: `admin`, `author`. Multipart fields:

| Field      | Type    | Description                                     |
| ---------- | ------- | ----------------------------------------------- |
| `fileName` | text    | Original filename                               |
| `start`    | integer | Inclusive start offset                          |
| `end`      | integer | Exclusive end offset                            |
| `size`     | integer | Total file size                                 |
| `chunk`    | binary  | Chunk data; its length must equal `end - start` |
| `batch_id` | text    | Stable ID that is unique for the upload         |

The server configuration limits file size, chunk size, and concurrent uploads.
The backend derives the file type from the filename, and it must be present in
the server-side MIME allowlist. After the last chunk, the media record is
created synchronously. Image variants are generated in the background and
report their status through SSE.

SVG files are accepted as `image/svg+xml` and can be selected for image node
blocks. They are kept in their original vector form; no raster dimensions or
generated image variants are created for SVG files.

## Node templates

| Method   | Path                               | Access            |
| -------- | ---------------------------------- | ----------------- |
| `GET`    | `/api/content/node/templates`      | `admin`, `author` |
| `POST`   | `/api/content/node/templates`      | `admin`           |
| `PUT`    | `/api/content/node/templates/{id}` | `admin`           |
| `DELETE` | `/api/content/node/templates/{id}` | `admin`           |

```json
{
    "name": "quote",
    "data": {
        "author": "",
        "source": ""
    },
    "schema": [
        { "key": "author", "kind": "string", "default": "" },
        { "key": "source", "kind": "string", "default": "" },
        { "key": "featured", "kind": "boolean", "default": false }
    ]
}
```

`schema` defines the fields rendered by the admin UI and validates nodes that
reference the template through `template_id`. Supported kinds are `string`,
`text`, `boolean`, `number`, and `json`. The backend applies missing defaults
when a templated node is saved and rejects values with a different type for a
defined field. Extra fields remain supported for forward compatibility and are
rendered as inferred fields in the admin UI. Nodes without `template_id` remain
free-form for backwards compatibility.

## Users and roles

| Method   | Path                  | Access                                       |
| -------- | --------------------- | -------------------------------------------- |
| `GET`    | `/api/auth-role`      | `admin`                                      |
| `GET`    | `/api/auth-user`      | `admin`; own profile for authenticated users |
| `POST`   | `/api/auth-user`      | `admin`                                      |
| `PUT`    | `/api/auth-user/{id}` | `admin`                                      |
| `DELETE` | `/api/auth-user/{id}` | `admin`                                      |

Administrators can additionally filter users with `search`, `last_login`,
`created_after`, and `created_before`. Regardless of query parameters,
`author`, `user`, and custom roles receive only their own profile with the
fields `email`, `first_name`, `last_name`, and `username`.

User body:

```json
{
    "email": "ada@example.org",
    "username": "ada",
    "first_name": "Ada",
    "last_name": "Lovelace",
    "password": "secret",
    "role_id": 2
}
```

Passwords are hashed with Argon2 before storage and are never returned by GET
requests.

## Configuration

| Method | Path                 | Access  |
| ------ | -------------------- | ------- |
| `GET`  | `/api/configuration` | `admin` |
| `PUT`  | `/api/configuration` | `admin` |

PUT always updates the single configuration record with ID `1` and accepts a
partial JSON object:

```json
{
    "output_type": "ast",
    "mail_smtp": "smtp.example.org",
    "mail_port": 587,
    "mail_user": "cms@example.org",
    "mail_password": "secret",
    "mail_starttls": true,
    "notification_emails": ["editor@example.org"],
    "image_extensions": ["webp", "jpg"],
    "image_resolutions": [480, 960, 1440]
}
```

Allowed fields are `jwt_secret`, `output_type`, `mail_smtp`, `mail_port`,
`mail_user`, `mail_password`, `mail_starttls`, `notification_emails`,
`image_extensions`, and `image_resolutions`. GET responses never include
`jwt_secret` or `mail_password`. The backend reloads its runtime configuration
immediately after a successful update.

## Contact form and mail targets

### Public contact form

```http
POST /api/contact/target/{target-name}
```

```json
{
    "email": "ada@example.org",
    "name": "Ada",
    "subject": "Question",
    "text": "Message"
}
```

The path parameter is the unique name of a configured mail target. The backend
validates the email address and checks the text for spam. `name` is limited to
160 characters, `subject` to 255 characters, and `text` to 20,000 characters.

### Managing mail targets

| Method   | Path                        | Access            |
| -------- | --------------------------- | ----------------- |
| `GET`    | `/api/contact/targets`      | `admin`, `author` |
| `POST`   | `/api/contact/targets`      | `admin`           |
| `PUT`    | `/api/contact/targets/{id}` | `admin`           |
| `DELETE` | `/api/contact/targets/{id}` | `admin`           |

```json
{
    "name": "contact",
    "subject": "Contact request",
    "recipients": ["team@example.org"],
    "allow_html": false
}
```

## Plugins

Enabled Wasmtime plugins and their optional admin-panel metadata are listed at:

```http
GET /api/plugins
Authorization: Bearer <access-token>
```

This endpoint is available to admins and authors:

```json
[
    {
        "id": "example",
        "version": "0.1.0",
        "admin": {
            "entry": "admin/index.html",
            "menu": [
                {
                    "label": "Example",
                    "path": "/admin/plugins/example",
                    "icon": "bi-puzzle"
                }
            ]
        }
    }
]
```

Plugin-defined endpoints normally live below `/api/plugins/{plugin-id}`. Their methods, request
and response formats, and access roles are declared by the individual plugin. A route can be
public, restricted to one role such as `author`, or shared by multiple roles such as
`admin,author`. Explicit non-reserved root routes may also be enabled by the server administrator.

See [Plugins](plugins.md) for the manifest, runtime, security, and migration model.

## Server-Sent Events

SSE access uses two steps. First, an authenticated user requests a short-lived
UUID:

```http
POST /sse/generate-uuid
Authorization: Bearer <access-token>
```

The permitted roles are `admin`, `author`, `user`, and custom roles.

```json
{
    "uuid": "550e8400-e29b-41d4-a716-446655440000"
}
```

The event connection must then be opened from the same client IP:

```http
GET /sse?uuid=550e8400-e29b-41d4-a716-446655440000
Accept: text/event-stream
```

The UUID is bound to the client IP and, where applicable, the user ID. It
cannot be reused arbitrarily. The backend sends a `ping` event immediately
after establishing the connection. Further events contain JSON-serialized
status and error messages, such as upload and image-variant updates.

## Rate limits

The default configuration applies a global limit of ten requests per second.
The following routes have stricter limits:

- `/auth/*`: three requests per minute
- `POST /api/comments`: one request every three minutes
- `/api/contact/target/*`: one request every three minutes

Limits are applied using the resolved client IP. When running behind a reverse
proxy, its networks must be configured in `TRUSTED_PROXY_CIDRS` before the
backend will trust forwarded client-IP headers.
