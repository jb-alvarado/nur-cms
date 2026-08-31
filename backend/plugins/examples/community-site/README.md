# Community site plugin

This is a generic example of a public website implemented as an external `nur-cms` Wasm plugin. It has no
project-specific migration or data. It renders three public pages from published CMS content:

- `/` reads the English development article `first-article`.
- `/privacy` reads the English development page `privacy-policy`.
- `/events` lists published entries of type `event`.
- `/events/<slug>` renders a selected event.

The plugin uses the `nur:cms/content` import. The host accepts the familiar public entry-list query string and
always enforces `status=published`, so a plugin cannot render drafts accidentally.
The example selects the safe `html` content output mode, which renders GitHub-Flavored Markdown and escapes
embedded raw HTML. For demonstration purposes it restores only a narrow allowlist used by the example content:
`div` with a restricted `class`, `i`, and `img` with an HTTPS source and `alt` text. Other escaped HTML remains
escaped. A plugin that deliberately supports broader raw HTML should request `markdown` or `ast` and use a
properly configured HTML sanitizer. That policy remains the plugin author's responsibility.

## Build and run

```sh
rustup target add wasm32-wasip2
cargo build --manifest-path backend/plugins/examples/community-site/Cargo.toml --target wasm32-wasip2 --release
NUR_PLUGINS=community-site NUR_PLUGIN_ALLOW_ROOT_ROUTES=1 cargo run -p nur-cms
```

The `assets` directory is served by the host under `/plugins/community-site/assets/`. The manifest enables a
five-minute cache for the public pages; successful CMS API writes invalidate it immediately.

Only one enabled plugin should own `/`. To mount a site below another public path, change the route paths and
the navigation links in `src/lib.rs`.
