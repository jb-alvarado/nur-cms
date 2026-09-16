# Configuration

nur-cms reads exactly one TOML configuration file. It uses the first existing
file in this order:

1. the path passed with `--config`
2. `./nur-cms.toml`
3. `$XDG_CONFIG_HOME/nur-cms/nur-cms.toml`, or
   `$HOME/.config/nur-cms/nur-cms.toml`
4. `/etc/nur-cms/nur-cms.toml`

An explicit `--config` path disables fallback to the other locations. Files are
not merged. This makes it possible to run several instances, including under
different users:

```console
nur-cms --config /etc/nur-cms/site-a.toml
nur-cms --config /etc/nur-cms/site-b.toml
```

Packages also install the `nur-cms@.service` systemd template. The instance
name selects its configuration, working directory, and Unix account. For
example, `nur-cms@site-a.service` uses:

| Resource | Value |
| --- | --- |
| Configuration | `/etc/nur-cms/site-a.toml` |
| Working directory | `/var/lib/nur-cms/site-a` |
| User | `site-a` (with its primary group) |

The regular `nur-cms.service` follows the same layout with the working
directory `/var/lib/nur-cms/nur-cms`. The shared `/var/lib/nur-cms` parent is
owned by `root`, so one instance cannot create, rename, or remove another
instance's working directory.

Create these resources before enabling an instance:

```console
sudo useradd --system --user-group \
  --home-dir /var/lib/nur-cms/site-a --create-home site-a
sudo install -o root -g site-a -m 640 \
  /usr/share/nur-cms/nur-cms.toml /etc/nur-cms/site-a.toml
sudo systemctl enable --now nur-cms@site-a.service
```

Give the instance user access to the upload and plugin-storage directories
configured in its TOML file. Use distinct directories when instances must not
share files. A systemd drop-in may override `User`, `Group`, or
`WorkingDirectory` when an existing account or another directory layout is
preferred.

Create and validate configurations with the built-in commands:

```console
nur-cms config create ./nur-cms.toml
nur-cms config check ./nur-cms.toml
nur-cms config migrate ./nur-cms.toml
```

`config create` notices a `.env` in the current directory and offers to import
known legacy settings when run interactively. For scripts, select the source
explicitly:

```console
nur-cms config create ./nur-cms.toml --from-env .env
nur-cms config create ./nur-cms.toml --from-environment
nur-cms config create ./nur-cms.toml --no-env
```

Unit conversions during import must be exact. For example, a byte value that
cannot be represented as a whole MB is rejected. Imported summaries list only
variable names and never values.

The packaged `/usr/share/nur-cms/nur-cms.toml` is a template. DEB and RPM
installations copy it to `/etc/nur-cms/nur-cms.toml` only when that file does
not exist, so package upgrades do not replace the instance configuration. The
package post-install step runs `config migrate`; migrations create a backup
before they rewrite an older configuration. Source and container installations
can run the same command explicitly.

On DEB upgrades, previously running `nur-cms@…` instances are restarted one at
a time. Each restart waits until the instance has initialized and bound its
listener. If an instance does not become ready, the upgrade stops restarting
instances so the remaining processes continue to serve with the previous
binary until the problem is resolved and the package is configured again.

When upgrading an older package, the installer imports known settings from
`/home/nur-cms/.env` if the default TOML configuration does not exist yet. The
legacy file remains untouched and can be removed after the generated TOML has
been checked.

Only the development bootstrap switches remain environment variables:

```dotenv
NUR_DEV_AUTO_ADMIN=1
NUR_DEV_SEED_DATABASE=1
```

They are intentionally excluded from TOML and from `.env` imports.
The server loads `.env` from its current directory at startup so these switches
remain available during local development.

## Public entry cache

Public content entry, list, and facet responses are cached in memory. The cache
stores completed JSON responses and is local to one CMS process. Successful
content, media, locale, and configuration changes invalidate it immediately.

| TOML key | Default | Description |
| --- | ---: | --- |
| `entry_cache.enabled` | `true` | Enables the cache. |
| `entry_cache.capacity` | `512` | Maximum number of cached responses. |
| `entry_cache.time_to_idle_minutes` | `30` | Expires an inactive response. |
| `entry_cache.time_to_live_hours` | `24` | Absolute maximum response lifetime. |

## Video processing

Video uploads are stored immediately and transcoded by durable background jobs.
Install FFmpeg, including `ffprobe`, on every CMS instance that runs workers.
FFmpeg 6 or newer is recommended. Enabled profiles are checked against the
encoders reported by `ffmpeg -encoders`.

| TOML key | Default | Description |
| --- | ---: | --- |
| `video.processing_concurrency` | `1` | Concurrent jobs per process. |
| `video.processing_threads` | `2` | FFmpeg and filter threads per job. |
| `video.processing_timeout_minutes` | `60` | Timeout for one FFmpeg invocation. |
| `video.lease_seconds` | `120` | Renewable database worker lease. |
| `video.max_attempts` | `3` | Attempts after transient failures. |
| `video.max_duration_hours` | `8` | Maximum source duration. |
| `video.max_pixels` | `33177600` | Maximum source width multiplied by height. |
| `video.max_output_size_mb` | `0` | Post-encoding publication limit; `0` disables it. |
| `video.ffmpeg` | `ffmpeg` | FFmpeg executable path. |
| `video.ffprobe` | `ffprobe` | ffprobe executable path. |

Temporary files are written below `uploads/.processing/`. Keep this directory
on the same filesystem as the public upload directory and do not serve it with
a reverse proxy. The output-size check runs after encoding and therefore does
not limit temporary disk use.

Resumable upload coordination is process-local. Route all `/api/upload`
requests consistently to one process when instances share an upload directory.
Video workers may run on several instances because jobs use database leases.

## Plugins

Plugins are installed separately and must be listed in `plugins.enabled`. See
the [plugin documentation](plugins.md) for their package and manifest format.

Runtime limits use seconds and MB in TOML. Private plugin storage must not
overlap `uploads.directory`. Upload sessions use
`uploads.session_lifetime_hours`, while requested plugin file-link lifetimes
are capped by `plugins.storage.file_links_max_age_hours`.

Browser-side plugin code is disabled unless
`plugins.allow_admin_components = true`. Root-scoped plugin routes are disabled
unless `plugins.allow_root_routes = true`.

## Public URL and comment moderation

`server.public_url` is the canonical URL exposed to plugins and used for
comment-notification links. HTTPS is required, except for local HTTP on
`localhost` or `127.0.0.1`. Credentials, queries, and fragments are rejected.
`comments.moderation_token_lifetime_days` controls the lifetime of one-time
comment approval and rejection links.
