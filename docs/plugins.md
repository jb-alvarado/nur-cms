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
and special files are not allowed in the public asset tree.

During development, nur-cms also searches `backend/plugins/examples`. Linux installations additionally
search `/usr/share/nur-cms/plugins` and `/var/lib/nur-cms/plugins`. Extra roots can be supplied with
`NUR_PLUGIN_DIR` using the platform path separator.

## Manifest

```toml
[plugin]
id = "example"
version = "0.1.0"
api_version = 1
cms_version = ">=0.16, <0.17"
module = "plugin.wasm"

[migrations]
directory = "migrations"

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
entry = "admin/index.html"

[[admin.menu]]
label = "Example"
path = "/admin/plugins/example"
icon = "bi-puzzle"
```

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

`GET /api/plugins` returns enabled plugin and admin metadata to admins and authors. Admin assets and
sandboxed admin pages are reserved by the manifest but are not loaded into the Vue application yet.

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

## Static assets

Files below the manifest's asset directory are served at `/plugins/<plugin-id>/assets/`. This namespace is
reserved and cannot also be used by a plugin route. Assets are treated as public files and are validated at
startup to prevent links outside the plugin package.

## Route cache

Caching is disabled unless a plugin declares `[cache]`. It creates a separate in-memory Moka cache for that
plugin. `NUR_PLUGIN_CACHE_MEMORY_LIMIT` is a shared approximate byte budget divided equally among all
configured plugin caches. Each cache is constrained by both its share of that budget and the manifest's
`max_entries`. Only public `GET` and `HEAD` routes may be cached. All eligible routes are cached by default;
set `cache = false` on an individual route to opt out.

The cache key includes the route, method, path, query string, and all request headers forwarded to the plugin.
Cached routes skip Wasm instantiation on a hit. The cache is local to one nur-cms process and is cleared on
restart. Every successful writing request to `/api` (`POST`, `PUT`, `PATCH`, or `DELETE`) invalidates all
plugin route caches in that process.

## Migrations

Core migrations continue to use SQLx's `_sqlx_migrations`. Plugin migrations are tracked separately
in `_plugin_migrations` with `(plugin_id, version)` as their primary key, so every plugin has its own
independent `0001`, `0002`, and later versions.

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
explicitly qualified core objects. Only install plugins from trusted sources.

## Runtime limits

Each request gets a fresh Wasmtime store with no inherited environment, network, standard input, or
filesystem access. Execution is bounded by fuel, epoch interruption, host-call limits, database timeouts,
linear-memory limits, body limits, and a global concurrency semaphore. Requests that arrive while all plugin
execution slots are occupied receive `503 Service Unavailable` instead of entering an unbounded queue. An
outer HTTP deadline also covers request preparation and response handling. Wasmtime runs on Tokio's blocking
pool so plugin code does not block an asynchronous Axum worker.

See [configuration.md](configuration.md) for all runtime variables and defaults.

## Examples

The minimal example is in [`backend/plugins/examples/echo`](../backend/plugins/examples/echo). The
[`community-site`](../backend/plugins/examples/community-site) example demonstrates public content queries,
root routes, static assets, and route caching. Build an example with:

```sh
rustup target add wasm32-wasip2
cargo build --manifest-path backend/plugins/examples/echo/Cargo.toml --target wasm32-wasip2 --release
```

Then run nur-cms with:

```sh
NUR_PLUGINS=echo NUR_PLUGIN_ALLOW_ROOT_ROUTES=1 cargo run -p nur-cms
```
