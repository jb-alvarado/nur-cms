# Vue admin plugin example

This proof of concept implements a nur-cms admin page as a Vue 3 Single File Component. Vue is an
implementation detail of the plugin; the public boundary with nur-cms is the `nur-cms-vue-admin` Custom
Element and its `context` property.

Build the independently versioned admin bundle with:

```bash
cd backend/plugins/examples/vue-admin/admin
npm install
npm run build
```

This creates `admin/dist/index.js`, a standalone browser ESM bundle that includes Vue and the component's
styles without requiring Node.js globals at runtime. The styles are injected into the Custom Element's Shadow
DOM. Build the small example backend component with:

```bash
cargo build --manifest-path backend/plugins/examples/vue-admin/Cargo.toml --target wasm32-wasip2 --release
```

Run nur-cms with `NUR_PLUGINS=vue-admin` and `NUR_PLUGIN_ALLOW_ADMIN_COMPONENTS=1`. The manifest exposes the
admin page at `/admin/plugins/vue-admin/overview`. nur-cms dynamically imports `admin/dist/index.js`, creates
the element named by the manifest, and assigns the authenticated plugin context. The button calls the
protected `/api/plugins/vue-admin/ping` route through `context.request()`; it never accesses an auth token.

The plugin and its Vue bundle are built independently from the nur-cms frontend. Installing or replacing the
built plugin files therefore does not require rebuilding the main frontend. Module Federation is not used.
