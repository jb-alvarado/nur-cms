# Plugins

nur-cms plugins are WebAssembly components executed by Wasmtime. Plugins can provide HTTP routes,
run independently versioned database migrations, expose static assets, and advertise future admin-panel
pages and menu items. A plugin is loaded only when its ID is listed in `NUR_PLUGINS`.

## Package layout

The package directory name must match the plugin ID:

```text
example/
├── plugin.toml
├── plugin.wasm
├── assets/
│   └── site.css
├── migrations/
│   └── 0001_create_tables.sql
└── admin/
    └── index.html
```

The module, asset, and migration directories must resolve inside the package directory. Symbolic links
and special files are not allowed in the public asset tree. Production plugin directories must not be
writable by the nur-cms runtime user or by any account that can upload content through the application.

During development, nur-cms also searches `backend/plugins/examples`. Linux installations additionally
search `/usr/share/nur-cms/plugins` and `/var/lib/nur-cms/plugins`. Extra roots can be supplied with
`NUR_PLUGIN_DIR` using the platform path separator.

## Manifest

```toml
[plugin]
id = "example"
name = "Example"
version = "0.1.0"
api_version = 1
cms_version = ">=0.16, <0.17"
module = "plugin.wasm"

[migrations]
directory = "migrations"

[mail]
targets = ["contact", "orders"]
dynamic_recipient_targets = ["orders"]
trusted_template_targets = ["orders"]

[[storage.directories]]
id = "documents"
path = "documents/{year}/{month}"
extensions = ["pdf", "docx", "odt"]
visibility = "public"
upload = "none"
access = "admin,author"

[assets]
directory = "assets"

[cache]
ttl_seconds = 300
max_entries = 128

[[routes]]
id = "list"
method = "GET"
path = "/api/plugins/example/items"
access = "public"

[[routes]]
id = "write"
method = "POST"
path = "/api/plugins/example/items"
access = "admin,author"

[[routes]]
id = "author-preview"
method = "GET"
path = "/api/plugins/example/preview"
access = "author"

[admin]
entry = "admin/index.js"
element = "nur-cms-example"
access = "admin,author"
styles = ["admin.css"]

[[admin.menu]]
label = "Items"
labels = { de = "Einträge", en = "Items" }
path = "/admin/plugins/example/items"
icon = "bi-puzzle"
access = "admin,author"

[[admin.menu]]
label = "Statistics"
labels = { de = "Statistiken", en = "Statistics" }
path = "/admin/plugins/example/statistics"
icon = "bi-bar-chart-line"
access = "admin"
```

`name` is an optional, human-readable display name for the admin menu. If it is omitted, the plugin ID is used.

`access` is either `public` or a comma-separated list of roles. Multiple roles use OR semantics:
`admin,author` permits admins and authors. `public` cannot be combined with another role. Public routes
never receive an authenticated user identity; protected routes receive the user ID and granted role names.

Supported methods are `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, and `OPTIONS`. Route IDs must
be unique within a plugin, and method/path combinations must be unique across all enabled plugins.

## Routes

The default namespace is `/api/plugins/<plugin-id>`. Routes elsewhere, including `/`, require
`NUR_PLUGIN_ALLOW_ROOT_ROUTES=1`. Plugins can never register routes below these reserved prefixes:

- `/auth`
- `/api` except their own `/api/plugins/<plugin-id>` namespace
- `/admin`
- `/sse`
- `/uploads`

Wildcard catch-all routes are rejected. Plugin routes pass through the normal nur-cms client-IP,
logging, authorization, and rate-limit middleware.

Named route parameters such as `/events/{slug}` are percent-decoded by the host and passed to the
plugin in `request.path-params`. The original request path remains available as `request.path`.
Query parameters are not route parameters and remain available as the raw query string in
`request.query`.

The host performs authorization before invoking WebAssembly. Plugins never receive JWTs, cookies,
database credentials, or unrestricted request headers.

`GET /api/plugins` returns enabled plugin metadata to authenticated users. Admin metadata is included
only when the current user's role is listed in the component's `access` declaration. Its `menu` contains
only the entries available to that role.

## Admin web components

An optional `[admin]` section can add pages to the CMS admin interface. `entry` is a JavaScript module,
`element` is the custom element it registers, `access` is a comma-separated list of authenticated CMS
roles, and `styles` is an optional list of CSS files. `access` defaults to `admin,author`. All paths are
relative to the declared asset directory. `entry` and `element` are required when an admin menu item is
declared. The element name must be lowercase and contain a hyphen. Set
`NUR_PLUGIN_ALLOW_ADMIN_COMPONENTS=1` to explicitly permit browser-side plugin code.

Each `[[admin.menu]]` may have its own `access` declaration. Without one it inherits `[admin].access`;
an explicit menu access list must be a subset of the component roles and uses the same OR semantics.
`labels` optionally maps lowercase locale codes to translated labels. The current locale is selected
reactively and missing translations fall back to `label`. The API filters menu entries by role, and the
admin router also refuses direct navigation to a path outside the user's visible menu namespaces. Nested
detail paths such as `/items/42` are allowed below an accessible `/items` menu path. Plugin API routes remain
the authoritative security boundary.

The CMS loads the module once when an admin route below `/admin/plugins/<plugin-id>` is visited, creates the
declared element, and supplies this `context` property before connecting it:

```js
class ExamplePlugin extends HTMLElement {
    connectedCallback() {
        this.unsubscribe = [
            this.context.onLocationChange((location) => this.renderRoute(location)),
            this.context.onLocaleChange(() => this.render()),
            this.context.onThemeChange(() => this.render()),
        ]
        this.load()
    }

    disconnectedCallback() {
        for (const unsubscribe of this.unsubscribe ?? []) unsubscribe()
    }

    async load() {
        const response = await this.context.request('/api/plugins/example/items')
        const data = await response.json()
        // Render with DOM APIs, a framework, or a custom-element library.
    }

    renderRoute(location) {
        // Switch list, detail, edit, or statistics views using location.relativePath.
        this.render(location)
    }

    render(location = this.context.location()) {
        // Read this.context.locale() and this.context.theme() while rendering.
    }
}

customElements.define('nur-cms-example', ExamplePlugin)
```

The complete context surface is:

```ts
type PluginAdminLocation = {
    path: string
    relativePath: string
    search: string
    hash: string
}

type PluginAdminContext = {
    pluginId: string
    roles: () => readonly string[]
    hasRole: (role: string) => boolean
    locale: () => string
    theme: () => 'light' | 'dark'
    location: () => PluginAdminLocation
    onLocationChange: (listener: (location: PluginAdminLocation) => void) => () => void
    onLocaleChange: (listener: (locale: string) => void) => () => void
    onThemeChange: (listener: (theme: 'light' | 'dark') => void) => () => void
    request: (path: string, init?: RequestInit) => Promise<Response>
    navigate: (path?: string) => Promise<void>
    notify: (variance: 'info' | 'success' | 'warning' | 'error', text: string) => void
}
```

`context.request(path, init)` uses the CMS access-token and refresh flow, but only permits requests inside
that plugin's `/api/plugins/<plugin-id>` namespace. No token, cookie, or complete user record is exposed.
`roles()` and `hasRole()` expose only role names for presentation decisions.

`location()` describes the current admin URL without requiring `window.location`; `relativePath` is relative
to `/admin/plugins/<plugin-id>`. `navigate()` accepts namespace-local relative paths, absolute paths inside
that namespace, and query/hash-only changes. External and foreign CMS paths are rejected. The custom element
stays connected while paths, queries, hashes, browser history, locale, or theme change. Subscribe to the
corresponding callbacks and invoke their returned cleanup functions when the element disconnects. Switching
to another plugin removes the old element and all host-side listeners.

`notify(variance, text)` displays a CMS notification. The context does not pass tokens as function arguments,
but same-origin plugin JavaScript executes with the administrator's browser privileges and must be considered
capable of reading or using credentials available to the CMS frontend.

Admin web components execute as JavaScript in the administrator's browser and therefore are trusted code.
The Wasmtime sandbox protects backend WasM modules only; install frontend-capable plugins from trusted
sources.

Plugin CSS is loaded into the admin document. It must use a plugin-specific prefix such as
`.nur-example-*` to avoid affecting the CMS or other plugins. Tailwind utility classes in runtime-loaded
plugin files are not part of the CMS build and are therefore not a stable styling API. CMS DaisyUI theme
variables remain available, including `--color-base-200`, `--color-primary`, and `--radius-box`. A custom
element that creates its own Shadow Root must bundle or inject the styles required inside that root.

## Public content access

The `nur:cms/content` WIT import provides `published-entries(query, output)`. Its argument uses the same
query parameters as the public `GET /api/content/entries` endpoint. `output` selects `markdown`, `ast`, or
`html` for node text. The host always enforces `status=published`; plugins cannot use this import to read
drafts or private content.

The built-in `html` output escapes raw HTML contained in Markdown. A plugin that deliberately supports raw
HTML should request `markdown` or `ast`, then choose and configure its own renderer and sanitizer. This keeps
the trust and allow-list policy in the application that ultimately embeds the result.

Host calls share the plugin request timeout, have a per-request call limit, and cannot return more bytes than
`NUR_PLUGIN_RESPONSE_BODY_LIMIT`. The return value is otherwise the normal JSON entry-list response.

## Plugin database access

The `nur:cms/database` WIT import lets a plugin use relational tables without receiving a connection string,
database credentials, filesystem access, or network access. `execute(statement)` runs one parameterized
statement; `transaction(statements)` runs one to 32 statements atomically. A failed statement rolls back the
whole transaction. Both return a `query-result` with `rows-affected`, column names, and rows, including
`RETURNING` values.

Values are typed as `boolean`, `integer` (`s64`), `float` (`f64`), `text`, `bytes`, or JSON encoded as
a string. A `null` value carries one of those target types, for example `Value::Null(NullType::Integer)`,
so PostgreSQL can type the bound parameter correctly. Bind every external value through `params`; do not
concatenate it into SQL:

```sql
-- migrations/0001_create_messages.sql
CREATE TABLE messages (
    id BIGSERIAL PRIMARY KEY,
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

```rust
use bindings::nur::cms::database::{self, Statement, Value};

let inserted = database::execute(&Statement {
    sql: "INSERT INTO messages (body) VALUES ($1) RETURNING id, body".into(),
    params: vec![Value::Text(user_input)],
})?;

let rows = database::execute(&Statement {
    sql: "SELECT id, body FROM messages WHERE id = $1".into(),
    params: vec![Value::Integer(42)],
})?;

database::transaction(&[
    Statement {
        sql: "UPDATE messages SET body = $1 WHERE id = $2".into(),
        params: vec![Value::Text("Updated".into()), Value::Integer(42)],
    },
    Statement {
        sql: "DELETE FROM messages WHERE id = $1".into(),
        params: vec![Value::Integer(43)],
    },
])?;
```

At runtime, each host call opens a database transaction and sets a transaction-local search path to the
plugin schema, for example `nur_plugin_example_plugin`. Runtime SQL is parsed with the PostgreSQL dialect
and accepts only one unqualified `SELECT`, `INSERT`, `UPDATE`, or `DELETE` statement. DDL, `SELECT INTO`,
data-modifying subqueries, multiple statements, comments, quoted or qualified identifiers, system relations,
table functions, and non-allow-listed functions are rejected. This deliberately small subset prevents a
request value from turning a plugin query into arbitrary SQL and keeps normal relation lookup inside the
plugin schema. Cast uncommon result types to `text` or JSON in the query; the host natively returns booleans,
integer and floating PostgreSQL types, `bytea`, JSON/JSONB, and text-like values.

The allow-listed scalar and aggregate functions are `abs`, `avg`, `ceil`, `ceiling`, `char_length`,
`coalesce`, `concat`, `count`, `floor`, `greatest`, `json_array_length`, `jsonb_array_length`,
`jsonb_typeof`, `least`, `length`, `lower`, `max`, `min`, `octet_length`, `round`, `substring`, `sum`,
`trim`, and `upper`. Additions belong in the host allow-list and its tests rather than in individual plugins.

The SQL subset and search path are runtime hardening, not a replacement for PostgreSQL permissions. Plugins
and their migrations are trusted server code: a migration can intentionally create views, functions, or
triggers with the application database user's privileges. Only install reviewed plugins, and do not grant the
CMS database role unnecessary operating-system or PostgreSQL administration privileges.

Database results are streamed and stopped at `NUR_PLUGIN_RESPONSE_BODY_LIMIT` or 10,000 rows, before a full
oversized result is retained in host memory. The normal plugin host-call limit and timeout apply to each
`execute` or `transaction` call. Database failures are logged by
nur-cms but returned to a plugin as generic errors, so connection details and SQL diagnostics do not reach
HTTP clients.

## Plugin mail access

The `nur:cms/mail` WIT import sends a message through a CMS-managed mail target without exposing SMTP
credentials:

Mail access is denied unless the target is explicitly declared in the plugin manifest. Dynamic recipients
require a second, target-specific capability:

```toml
[mail]
targets = ["contact", "orders"]
dynamic_recipient_targets = ["orders"]
trusted_template_targets = ["orders"]
```

Every entry in `dynamic_recipient_targets` and `trusted_template_targets` must also occur in `targets`. These
manifest permissions limit which mail targets and delivery modes the plugin may request; they do not expose
target addresses or SMTP credentials.

```rust
use bindings::nur::cms::mail::{self, ContentKind, Message};

mail::send(&Message {
    target: "contact".into(),
    recipient: None,
    name: "Example form".into(),
    reply_to: "sender@example.org".into(),
    subject: Some("A subject".into()),
    text: "A normal, sufficiently detailed message from the plugin form.".into(),
    content_kind: ContentKind::UserInput,
})?;
```

`target` selects an existing CMS mail target. The host uses the same address validation, message limits,
target lookup, HTML policy, reply-to handling, SMTP configuration, and error handling as
`POST /api/contact/target/{target}`. An unknown target, rejected address, spam message, or delivery failure
does not reveal SMTP or database details to a plugin route's HTTP client. The supplied `text` is used as the
message body without contact-form decoration; the selected target's `allow_html` setting still decides whether
HTML is preserved or stripped.

Use `ContentKind::UserInput` whenever the message body contains public free text. It applies the normal spam
evaluation. `ContentKind::TrustedTemplateHtml` skips that heuristic for HTML generated from a template controlled
by the plugin and is accepted only when the target is listed in `trusted_template_targets`. This permission does
not make interpolated values safe: escape all user-controlled values before inserting them into HTML. The mail
target must also have `allow_html` enabled, otherwise the host strips HTML before delivery.

By default, the fixed `recipients` configured on the selected mail target receive the message. An administrator
can explicitly enable `allow_dynamic_recipient` for a target. The plugin must additionally list that target in
`dynamic_recipient_targets`. It may then set `recipient` to one address;
that normalized and validated address replaces the fixed recipients for that message instead of being added to
them. The host does not accept recipient lists, CC, or BCC fields. Keep this permission disabled for targets that
do not need customer-specific delivery.

```rust
mail::send(&Message {
    target: "orders".into(),
    recipient: Some("customer@example.org".into()),
    name: "Order service".into(),
    reply_to: "shop@example.org".into(),
    subject: Some("Confirm your order".into()),
    text: "<p>Please confirm your order using the link in this message.</p>".into(),
    content_kind: ContentKind::TrustedTemplateHtml,
})?;
```

On a public plugin route, mail is limited independently per plugin route and client IP. The default interval
is three minutes and can be changed with `NUR_PLUGIN_PUBLIC_MAIL_INTERVAL_SECONDS`; the bounded in-memory
tracking capacity is configured with `NUR_PLUGIN_PUBLIC_MAIL_MAX_CLIENTS`. The first mail call in one public
HTTP request atomically reserves this route/IP window after the first message has passed validation. Up to three
mail calls may then run in that same request;
the fourth returns `rate-limited`. A second request for the same plugin route and IP remains blocked until the
window expires. Routes and client IPs have independent keys, and protected routes skip the public-IP window but
retain the same three-call request limit. A host adapter may invoke ordinary plugin functionality without a
client IP, but mail from a public route fails closed with `rate-limited` when no client IP is available. This is
not configurable, to prevent accidentally creating an unrestricted mail relay. The client IP is never exposed
to Wasm. Multiple SMTP deliveries are not atomic: if a later send fails, a previously delivered message cannot
be rolled back.

Input failures are returned as stable `bad-request` errors for unknown targets, invalid addresses, header-like
values, empty messages, and spam. Missing manifest permissions for the target, dynamic recipient, or trusted
template mode return `forbidden`. Request mail limits use `rate-limited`; SMTP, database, timeout, and other
internal delivery failures use a generic `failed` error without connection or configuration details.

## Public CMS URL

The read-only `nur:cms/configuration` import exposes only the canonical public URL:

```rust
use bindings::nur::cms::configuration;

let public_url = configuration::public_url()
    .ok_or_else(|| PluginError::Failed("public CMS URL is not configured".into()))?;
let confirmation_url = format!("{public_url}/orders/42/confirm");
```

The host reads `NUR_PUBLIC_URL`, trims whitespace and trailing slashes, and returns `none` when it is absent or
invalid. HTTPS is required except for HTTP on `localhost` and `127.0.0.1`. Reading this immutable value does not
consume a host-call slot. The Wasm process receives no environment variables, so this interface does not expose SMTP credentials,
database credentials, or other process configuration.

The database, mail, and storage imports were added while plugin API version 1 is still under development.
Rebuild version-1 WebAssembly components against the current WIT package before installing them with this
CMS build.

## Plugin storage

Plugins may write files only to directories declared in `[[storage.directories]]`. Directory IDs are
stable capabilities passed to the WIT storage API, and every path remains inside a namespace owned by
the plugin. `{year}` and `{month}` placeholders are expanded when a file is written. `write` returns
the concrete path; retain that path and pass it to `delete` later.

Directory IDs, paths, and visibility form a persistent storage contract. A plugin update may add
extensions, but it may not remove a directory, change its path or visibility, or remove a previously
allowed extension while files may still reference that declaration. Startup rejects such an update
instead of silently stranding existing files.

Public directories are stored below `STORAGE/plugins/<plugin-id>` and receive an
`/uploads/plugins/...` URL. Private directories require `NUR_PLUGIN_STORAGE` and never receive a
public URL. Public and private roots must be separate and non-overlapping.

Every directory must declare `extensions`. Supported passive formats are AVIF, CSV, DOC, DOCX, GIF,
JPEG, MP3, MP4, ODS, ODT, OGG, PDF, PNG, PPT, PPTX, RTF, TXT, WAV, WebM, WebP, XLS, and XLSX. Active
same-origin content such as HTML, SVG, XML, and JavaScript is rejected. Extension checks are
case-insensitive.

`NUR_PLUGIN_STORAGE_WRITE_LIMIT` limits one WIT or authenticated whole-body write.
`NUR_PLUGIN_STORAGE_QUOTA` limits the combined public and private bytes owned by one plugin.
Resumable uploads reserve their declared size before accepting chunks. Operations are serialized per
plugin; deployments with several CMS processes sharing storage should additionally enforce a
filesystem quota. Filenames are limited to 236 UTF-8 bytes so resumable-upload metadata remains valid
on filesystems with 255-byte path-component limits.

### Browser uploads and one-time links

Set `upload = "authenticated"` to expose a directory only through authenticated file routes, or
`upload = "link"` to let the plugin issue a short-lived public upload capability. The `access` roles
still control authenticated file management and which plugin routes may request a link; possession of
an issued link is the authorization for that one public upload. Store links only as briefly as needed
and never log them. `upload = "link"` works for both public and private storage: public describes the
resulting file's visibility, not whether the capability URL itself can be called.

```rust
let link = storage::create_upload_link(&UploadLinkRequest {
    directory: "documents".into(),
    filename: "application.pdf".into(),
    max_size: 10 * 1024 * 1024,
    expires_seconds: 15 * 60,
})?;
```

The returned URL supports the same resumable range protocol as the CMS media uploader:

1. Generate one stable, random `batch_id` of 7–128 ASCII letters, digits, `_`, or `-`.
2. Query `GET <link>?file_name=<name>&size=<bytes>&batch_id=<id>`. The response contains
   `received_ranges` and `complete`. When the server recovers a file finalized before an interrupted
   database update, `complete` is true and `file` contains the stored-file result.
3. Send each range with `POST <link>` as `multipart/form-data` fields `fileName`, `start`, `end`,
   `size`, `batch_id`, and binary `chunk`. Offsets are byte offsets and `end` is exclusive.
4. Repeat missing ranges. Intermediate responses contain the updated ranges; the final response is
   `{ "path": "...", "public_url": "..." }`.

The first valid status or chunk request binds the capability to its `batch_id`. Another batch cannot
take it over. The token is consumed only after the complete temporary file has been atomically moved
to its destination. Interrupted uploads can resume for `UPLOAD_TTL_SECONDS`, including after the
original link expiry once the upload was claimed. Upload sessions and the maximum requested link
lifetime both default to 48 hours and are configurable with `UPLOAD_TTL_SECONDS` and
`NUR_PLUGIN_FILE_LINK_MAX_AGE_HOURS`. Chunk size is limited by `MAX_CHUNK_SIZE`; total
size is limited by the link, `MAX_UPLOAD_SIZE`, and the per-plugin storage quota. Public and private
plugin files are not inserted into the CMS `media` table and receive no image or video processing.

Authenticated admin tooling can manage a directory without a one-time link:

- `POST /api/plugins/<plugin>/files/<directory>?filename=<name>` writes a complete request body.
- `GET /api/plugins/<plugin>/files/<directory>/download?path=<stored-path>` downloads a file.
- `DELETE /api/plugins/<plugin>/files/<directory>?path=<stored-path>` deletes a file.

These routes enforce the directory's `access` roles. A plugin normally records returned paths and any
additional metadata in its own database tables; listing and domain-specific file management remain a
plugin responsibility.

The CMS does not rely on `Content-Disposition` for this boundary because `/uploads` may be served by
an upstream web server. That server should preserve `X-Content-Type-Options: nosniff`. The admin UI
should open only known preview-safe formats inline and treat documents as downloads unless a dedicated
preview is used.

## Static assets

Files below the manifest's asset directory are served at `/plugins/<plugin-id>/assets/`. This namespace is
reserved and cannot also be used by a plugin route. Assets are treated as public files and are validated at
startup to prevent links outside the plugin package.

## Route cache

Caching is disabled unless a plugin declares `[cache]`. It creates a separate in-memory Moka cache for that
plugin. `NUR_PLUGIN_CACHE_MEMORY_LIMIT` is a shared approximate byte budget divided equally among all
configured plugin caches. Each cache is constrained by both its share of that budget and the manifest's
`max_entries`. Only public `GET` and `HEAD` routes may be cached. All eligible routes are cached by default;
set `cache = false` on an individual route to opt out. Cached routes reject request bodies because bodies
are not part of their HTTP cache identity.

The cache key includes the route, method, path, query string, and all request headers forwarded to the plugin.
Cached routes skip Wasm instantiation on a hit. The cache is local to one nur-cms process and is cleared on
restart. Every successful writing request to `/api` (`POST`, `PUT`, `PATCH`, or `DELETE`) invalidates all
plugin route caches in that process. A successful writing plugin route invalidates that plugin's cache as
well, including responses backed by its plugin-local database.

## Migrations

Core migrations continue to use SQLx's `_sqlx_migrations`. The plugin runtime owns the separate
`_plugin_migrations` journal. Its `runtime` scope tracks infrastructure migrations shipped by the
`nur_plugins` crate, while its `plugin` scope tracks each installed plugin independently by
`(plugin_id, version)`. Every plugin can therefore have its own `0001`, `0002`, and later versions
without colliding with runtime or core migrations.

Each plugin gets a PostgreSQL schema derived from its validated ID, for example:

```text
example-plugin -> nur_plugin_example_plugin
```

Before executing a migration, nur-cms sets that schema as the transaction-local `search_path`.
Each migration is executed in its own transaction while holding a plugin-specific PostgreSQL
advisory lock. Applied files are protected by SHA-256 checksums; changing or removing an applied
migration prevents startup. Down migrations are never run automatically.

Migration SQL is installation-time trusted code. The schema and tracking separation prevent normal
name and version collisions, but the application database user may still have permission to modify
explicitly qualified core objects. Runtime database access is intentionally more restricted, but both
server-side Wasm and migrations must be installed only from trusted sources.

## Runtime limits

Each request gets a fresh Wasmtime store with no inherited environment, network, standard input, or
filesystem access. Execution is bounded by fuel, epoch interruption, host-call limits, database timeouts,
linear-memory limits, body limits, and a global concurrency semaphore. Requests that arrive while all plugin
execution slots are occupied receive `503 Service Unavailable` instead of entering an unbounded queue. An
outer HTTP deadline also covers request preparation and response handling. Wasmtime runs on Tokio's blocking
pool so plugin code does not block an asynchronous Axum worker. Epoch interruptions are reported as
`504 Gateway Timeout`. Plugin responses may contain at most 64 headers, 8 KiB per header value, and 64 KiB
of headers in total.

Compiled components are cached on disk by default. The cache key includes the component bytes, Wasmtime
version, target, and relevant compiler configuration, so changed or incompatible components are compiled
again automatically. The first load still compiles a component; later process starts reuse the cached native
artifact. Production services and containers should set `NUR_PLUGIN_COMPILATION_CACHE_DIR` to a persistent,
application-owned directory.

See [configuration.md](configuration.md) for all runtime variables and defaults.

## Examples

The minimal example is in [`backend/plugins/examples/echo`](../backend/plugins/examples/echo). The
[`community-site`](../backend/plugins/examples/community-site) example demonstrates public content queries,
root routes, static assets, and route caching. The
[`vue-admin`](../backend/plugins/examples/vue-admin) example demonstrates an independently built Vue Custom
Element that is loaded into the admin interface at runtime. Build a Rust example with:

```sh
rustup target add wasm32-wasip2
cargo build --manifest-path backend/plugins/examples/echo/Cargo.toml --target wasm32-wasip2 --release
```

Then run nur-cms with:

```sh
NUR_PLUGINS=echo NUR_PLUGIN_ALLOW_ROOT_ROUTES=1 NUR_PLUGIN_ALLOW_ADMIN_COMPONENTS=1 cargo run -p nur-cms
```
