# Configuration reference

Copy [`assets/.env.example`](../assets/.env.example) to your `.env` file and adjust the values for
your environment. Command-line options are listed by `cargo run -- --help`.

## Public entry cache

Public content entry, list, and facet responses are cached in memory by default. The cache stores
the completed JSON response, so it avoids repeated database queries, Markdown rendering, and JSON
serialization. It is local to one CMS process and applies only to unauthenticated requests.

Successful content, media, locale, and configuration changes invalidate all cached public entry
responses immediately.

| Variable | Default | Description |
| --- | --- | --- |
| `NUR_ENTRY_CACHE` | `1` | Set to `0` to disable the cache. |
| `NUR_ENTRY_CACHE_CAPACITY` | `512` | Maximum number of cached responses shared by entry, list, and facet requests. |
| `NUR_ENTRY_CACHE_TTI_SECONDS` | `1800` | Expire a response after this many seconds without access. |
| `NUR_ENTRY_CACHE_TTL_SECONDS` | `86400` | Expire a response after this many seconds even if it is accessed. |

Time-to-idle is refreshed on each cache hit. Time-to-live is an absolute maximum lifetime and is
not extended by access.

## Plugins

Plugins are installed separately and must be explicitly enabled. See the [plugin documentation](plugins.md)
for the package layout, manifest format, migration behavior, and HTTP interface.

| Variable | Default | Description |
| --- | --- | --- |
| `NUR_PLUGINS` | empty | Comma-separated IDs of plugins to enable. |
| `NUR_PLUGIN_DIR` | platform defaults | Additional plugin roots separated by the platform path separator. |
| `NUR_PLUGIN_ALLOW_ROOT_ROUTES` | `0` | Set to `1` to permit non-reserved routes outside the plugin API namespace. |
| `NUR_PLUGIN_FUEL` | `1000000` | Wasmtime fuel available to each request. |
| `NUR_PLUGIN_MEMORY_LIMIT` | `67108864` | Maximum linear memory per plugin request in bytes. |
| `NUR_PLUGIN_MODULE_SIZE_LIMIT` | `67108864` | Maximum size of a plugin WebAssembly component in bytes. |
| `NUR_PLUGIN_TIMEOUT_MS` | `5000` | Wall-clock execution limit per plugin request. |
| `NUR_PLUGIN_MAX_CONCURRENCY` | `8` | Maximum number of concurrent plugin executions. |
| `NUR_PLUGIN_MAX_HOST_CALLS` | `16` | Maximum number of CMS host-interface calls during one plugin request. |
| `NUR_PLUGIN_REQUEST_BODY_LIMIT` | `1048576` | Maximum request body passed to a plugin in bytes. |
| `NUR_PLUGIN_RESPONSE_BODY_LIMIT` | `4194304` | Maximum HTTP response or individual CMS host response accepted from a plugin in bytes. |
