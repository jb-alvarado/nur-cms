# Echo plugin

Build the example from the repository root:

```sh
rustup target add wasm32-wasip2
cargo build --manifest-path backend/plugins/examples/echo/Cargo.toml --target wasm32-wasip2 --release
```

Enable it for a development run:

```sh
NUR_PLUGINS=echo NUR_PLUGIN_ALLOW_ROOT_ROUTES=1 NUR_PLUGIN_ALLOW_ADMIN_COMPONENTS=1 cargo run -p nur-cms
```

The plugin demonstrates a public route, an author-only route, a route shared by admins and
authors, a root-level route, admin menu metadata, and a plugin-local migration.

`assets/admin.js` also demonstrates an admin web component. It receives the CMS-provided
authenticated `context.request()` function and calls the plugin's protected `editor` route without
implementing a separate login or token-refresh flow.
