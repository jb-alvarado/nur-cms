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
