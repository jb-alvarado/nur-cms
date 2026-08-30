# Plugins

nur-cms plugins are WebAssembly components executed by Wasmtime. Plugins can provide HTTP routes,
run independently versioned database migrations, and advertise future admin-panel pages and menu
items. No plugin is loaded merely because it is present on disk: its ID must be listed in
`NUR_PLUGINS`.

## Package layout

```text
example/
├── plugin.toml
├── plugin.wasm
├── migrations/
│   └── 0001_create_tables.sql
└── admin/
    └── index.html
```

The module and migration directory must resolve inside the package directory. Symlinks cannot be
used to escape the package.

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
`admin,author` permits admins and authors. `public` cannot be combined with another role.

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

The host performs authorization before invoking WebAssembly. Plugins receive only the authenticated
user ID and role names, never JWTs, cookies, database credentials, or unrestricted request headers.

`GET /api/plugins` returns enabled plugin and admin metadata to admins and authors. Admin assets and
sandboxed admin pages are reserved by the manifest but are not loaded into the Vue application yet.

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
filesystem access. Execution is bounded by fuel, epoch interruption, linear-memory limits, body
limits, and a global concurrency semaphore. Wasmtime runs on Tokio's blocking pool so plugin code
does not block an asynchronous Axum worker.

See [configuration.md](configuration.md) for all runtime variables and defaults.

## Example

The complete example is in [`backend/plugins/examples/echo`](../backend/plugins/examples/echo). Build it with:

```sh
rustup target add wasm32-wasip2
cargo build --manifest-path backend/plugins/examples/echo/Cargo.toml --target wasm32-wasip2 --release
```

Then run nur-cms with:

```sh
NUR_PLUGINS=echo NUR_PLUGIN_ALLOW_ROOT_ROUTES=1 cargo run -p nur-cms
```
