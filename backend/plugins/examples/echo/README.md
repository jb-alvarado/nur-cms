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

The plugin demonstrates public and protected routes, a root-level route, admin menu metadata,
and a plugin-local migration. The protected `POST /api/plugins/echo/database` endpoint inserts
and reads `echo_messages`; `POST /api/plugins/echo/rollback` demonstrates an atomic rollback.
`POST /api/plugins/echo/mail` reads `configuration.public-url()`, creates an absolute confirmation
link, and sends two messages through the configured `contact` CMS mail target: one to its fixed
merchant recipients and one exclusively to the customer address supplied as the plain request body.
Set `NUR_PUBLIC_URL` and enable `allow_dynamic_recipient` on that target before testing it. Both calls
share one public plugin-mail rate-limit reservation and use the normal CMS validation:

```sh
curl -X POST --data 'customer@example.org' http://127.0.0.1:8777/api/plugins/echo/mail
```

`assets/admin.js` also demonstrates an admin web component. It receives the CMS-provided
authenticated `context.request()` function and calls the plugin's protected `editor` route without
implementing a separate login or token-refresh flow. Its overview and admin-tools pages demonstrate
role-specific and localized menu entries, namespace-safe navigation, relative locations, roles, and
live locale and theme updates without remounting the custom element.
