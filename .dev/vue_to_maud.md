# Idea: Replace the Vue admin UI with Maud

Status: Discussion proposal, with no commitment to implementation or timeline.
The notice in [README.md](README.md) applies.

## Goal and recommendation

The CMS administration could generate its HTML pages on the server using Rust,
Axum, and Maud. For the core interface, ordinary URLs, server-side view models,
and form actions would replace Vue Router and Pinia. The JSON API remains
available to external clients, the public example frontend, and existing plugins.

Recommendation: build a fully server-rendered baseline first, then enhance it
with small TypeScript modules where needed. Maud is an HTML templating library;
it does not replace browser interactions or state management for editors,
uploads, or live progress. Tailwind and DaisyUI could preserve the existing
appearance. A CSS/JavaScript build therefore remains useful even when the core
interface no longer requires Vue.

Maud provides `html!`, reusable rendering functions, and integration with Axum
responses through its `axum` feature. Dynamic text is escaped; `PreEscaped`
bypasses this protection and requires a clearly defined trust boundary.
See the [Maud documentation and source](https://docs.rs/maud/latest/src/maud/lib.rs.html).

## Current project structure

- `frontend/src/router/index.ts` defines the admin pages under `/admin/`.
- `frontend/src/stores/auth.ts` stores access and refresh tokens in the browser;
  `frontend/src/composables/authFetch.ts` sets Bearer headers and refreshes tokens.
- `frontend/src/components/SseHandler.vue` handles `/sse?uuid=…`, notifications,
  and media progress, including reconnection.
- `crates/app/build.rs` currently builds the frontend in release mode. The
  application serves the built assets through `crates/app/src/serve/routes.rs`.
- Plugins run as Wasmtime components. Manifests, role filters, and namespaces
  already exist; each plugin frontend loads as a Web Component and receives a
  `PluginAdminContext`.

The authoritative plugin contracts are documented in
[docs/plugins.md](../docs/plugins.md),
[manifest/mod.rs](../crates/plugins/src/manifest/mod.rs), and
[plugin.wit](../crates/plugins/wit/nur-cms-plugin/plugin.wit).

## Core interface architecture

A dedicated workspace crate at `crates/admin/`, with the proposed package name
`nur_admin`, groups view models, Maud components, pages, and layouts. Its library
remains independent of Axum, SQLx, Wasmtime, and `nur_core`.
Production admin handlers and the router live in a separate admin module in
`crates/app/`: they load data through `nur_core` and `nur_plugins`, construct
small view models, and pass them to the rendering functions. This keeps HTTP
integration separate from templates. `crates/core/` contains shared business
logic, data access, and authorization; `crates/plugins/` retains manifest
validation and Wasmtime execution.

API and admin handlers should call the same business operations. If an operation
currently lives directly inside an API handler, first extract it into a shared
function. The admin interface does not need to call its own JSON API over HTTP
and must not introduce duplicate validation logic.

- Layout functions render navigation, the header, alerts, and the content area.
- View models contain only the data and permissions needed by the page.
- `GET` renders pages; forms that change data use dedicated action routes.
  Successful actions return a `303` redirect. Errors redisplay the form with
  its submitted values and translated validation messages.
- Search, filters, sorting, and pagination are represented in the URL.
- English and German translations, themes, responsive navigation, and accessible
  forms remain part of the shared interface.
- JavaScript handles media selection, upload progress, Markdown previews,
  video/HLS, SSE, and drag-and-drop where needed. Editors remain clearly scoped
  browser components with a documented contract.

HTML fragments could be loaded dynamically later. A prototype should establish
whether htmx or small custom modules are more suitable. Full-page and fragment
requests must enforce the same permissions and handle the same error cases.

## Development workflow from the shared discussion

The [shared conversation](https://chatgpt.com/share/6ac8ef99-c5e4-83eb-b257-8f13bc78ff1d)
suggests a component library that can be developed independently. An optional
binary in the same package is sufficient; a separate development crate is not
required:

```text
crates/admin/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── bin/dev.rs
    ├── components/
    ├── layouts/
    ├── pages/
    └── view_models/
frontend/
└── src/                  # eventually CSS and small TypeScript modules
```

The proposed Cargo configuration is shown below. Dependencies and versions would
need to be aligned with the workspace during implementation:

```toml
[features]
default = []
dev-server = ["dep:axum", "dep:tokio"]

[[bin]]
name = "nur-admin-dev"
path = "src/bin/dev.rs"
required-features = ["dev-server"]
```

Maud is a regular dependency without its Axum feature; Axum and Tokio are optional
dependencies for the development binary. In production, `crates/app/` converts
rendered markup into an HTML response and depends on `nur_admin` with
`default-features = false`. `required-features` tells Cargo to skip the development
binary unless the feature is enabled. This feature is independent of debug and
release profiles; a build with `--all-features` would also enable it.
See [Cargo targets](https://doc.rust-lang.org/cargo/reference/cargo-targets.html).

The development binary runs locally only and uses fixed sample data to display
buttons, tables, navigation, forms, and complete pages under `/__dev/…`.
Error, empty, loading, role, language, and theme states should be directly
selectable. This preview requires neither PostgreSQL nor the Wasmtime runtime.
Actual authentication and plugin integration are also tested in the complete
CMS; the preview does not replace those checks.

Two processes form the development workflow:

```bash
# Start the asset server from the repository root as before
npm run dev

# Proposed binary, once the new crate exists
cargo run -p nur_admin --features dev-server --bin nur-admin-dev
```

A Rust watcher can rebuild and restart the second process when files in
`crates/admin/src` change. A live-reload mechanism included only in development
then reloads the page. CSS and TypeScript use Vite HMR; changed Maud markup still
requires Rust compilation. The small dependency graph reduces the scope of the
build but does not guarantee a particular build time.

In development, the layout includes the Vite client and asset entry point. In
production, it reads hashed filenames and associated CSS files from the Vite
build manifest; `/admin/` remains the base path. Development URLs must not appear
in release HTML. See [Vite backend integration](https://vite.dev/guide/backend-integration).
Tailwind must explicitly include Rust files as sources, for example through an
`@source` directive with a path relative to the CSS file. Dynamically assembled
classes need a fixed mapping. See [Tailwind source detection](https://tailwindcss.com/docs/detecting-classes-in-source-files).

htmx for server fragments and Alpine.js for local UI state are additional options
from the discussion. Add them only when there is a concrete need; the existing
plugin host remains independent of them. Fragment replacement must not
unexpectedly destroy connected plugin elements or unsaved editor state.

## Authentication for server-rendered pages

An ordinary browser page request does not send a Bearer token from `localStorage`.
Replacing Vue templates with Maud is therefore insufficient on its own.

Proposal: add an admin session with a random, opaque ID in an `HttpOnly` cookie,
using `Secure` in production and an appropriate `SameSite` setting. The server
manages the associated authentication and token refresh. The session must
integrate with the existing login, verification, refresh rotation, and logout
rules, including revocation and expiry. Tokens are never written into HTML or URLs.

`/auth` and Bearer authentication under `/api` remain compatible. A narrowly
scoped adapter for admin browser requests uses the session and the same
authentication and authorization functions. Cookie authentication should not be
enabled indiscriminately for all existing API routes. Session requests that
change data require CSRF protection and origin validation, including plugin
actions. Session storage, cleanup, and coordination of concurrent refreshes must
be defined before the pilot runs; do not hold locks across network or database
awaits.

For `/sse`, retain the existing UUID mechanism initially and integrate it with
the new session flow. Reconnection, logout, expiry, and progress displays must
work equivalently outside Vue.

## Plugins: shared menus, two rendering modes

### Preserve existing Web Components

The existing `[admin].entry`, `element`, `styles`, `access`, and `[[admin.menu]]`
fields remain valid. Maud renders the admin shell; a small framework-independent
host module loads the plugin module and sets its `context` before inserting the
custom element.

Preserve the complete existing `PluginAdminContext`: requests, navigation, roles,
language, theme, notifications, and media selection. `request()` uses the session
adapter without exposing tokens to the plugin. For navigation within a plugin,
the host keeps the element connected, updates browser history, and triggers
`onLocationChange`. Language and theme changes also trigger the existing
callbacks. Switching plugins cleans up the element, subscriptions, and any open
media selection. Browser back/forward navigation and direct access to detail
pages are part of the contract.

A separately built Vue custom element can therefore continue to work. Vue would
be removed from the CMS core interface but could remain an internal implementation
choice for a plugin. The existing `plugins.allow_admin_components` setting
continues to govern browser code, which still runs with the privileges of the
admin page.

### Add server-rendered plugin pages

For new plugins, introduce an explicit mode: `renderer = "server"`. Existing
Web Component manifests retain their current mode when this field is absent.
In server mode, `entry` and `element` are not required; instead, the plugin
declares admin routes. Example of a **new manifest extension that is not
currently supported**:

```toml
[admin]
renderer = "server"
access = "admin,author"

[[admin.menu]]
label = "Items"
labels = { de = "Einträge", en = "Items" }
path = "/items"
icon = "bi-puzzle"
access = "admin,author"

[[admin.routes]]
id = "admin-items"
method = "GET"
path = "/items"
access = "admin,author"

[[admin.routes]]
id = "admin-item-save"
method = "POST"
path = "/items/{id}"
access = "admin"
```

The host maps these routes exclusively to `/admin/p/<plugin-id>/…`. The existing
reservation of `/admin` for ordinary plugin routes remains in place; admin
routes use a separate registration path controlled by the CMS. Menu paths link
to matching GET pages. Subpages and actions receive their own explicit route
declarations and permissions.

Execution could use the existing WIT export `http-handler.handle`: `route-id`,
method, path parameters, query, body, and a verified identity already exist.
The plugin can generate its content fragment with Maud and return it as
`text/html`. It does not provide a complete admin shell. The host adds the layout,
navigation, and notifications. The prototype must establish whether the current
request contract covers language and all form cases; if new WIT types are needed,
introduce explicit versioning.

Successful form actions can return a `303` redirect; the host accepts only
validated targets within that plugin's admin namespace. Errors may return a form
fragment with an appropriate status. Do not forward session cookies, credentials,
or arbitrary request headers to Wasm.

### Menu and security contract

- A shared menu resolver supplies core and plugin entries for the current
  identity, reusing the existing role filters.
- `access` retains OR semantics; menu and route permissions may only narrow the
  parent admin permissions. Admin pages are never `public`.
- Labels are escaped as text; `labels` uses the current language with a fallback
  to `label`. Icons are validated names from an allowed set.
- Only enabled plugins with a permitted UI mode appear. An empty or invalid
  plugin menu must not break the core menu.
- Direct GETs, subpages, POSTs, and fragment requests also require server-side
  authorization. Hiding a menu entry does not restrict access.
- Paths are normalized and confined to the plugin's own namespace; traversal,
  other plugins' paths, and external redirects are rejected.
- Personalized admin responses use `Cache-Control: no-store` and bypass public
  plugin caches. Runtime, body, and header limits continue to apply. A plugin
  failure displays an error within its own content area.

Server-rendered plugin HTML is another trust boundary: the Wasm sandbox does not
prevent browser attacks through returned markup. For the new server mode, apply
an HTML allowlist before embedding content. Remove scripts, event attributes,
dangerous URLs, and uncontrolled attributes that trigger dynamic loading. Forms
and links may use only permitted targets; the host adds CSRF fields to permitted
forms that change data. Plugin CSS also needs a defined trust contract. Only the
validated fragment may enter the Maud layout through `PreEscaped`. Browser code
remains possible exclusively through the explicitly permitted Web Component mode.

The manifest extension must update validation, metadata under `/api/p`, generated
TypeScript types, documentation, and examples together. Older CMS versions reject
the new fields because of `deny_unknown_fields`; new server plugins therefore
need an appropriate minimum `cms_version`. Existing manifests and Wasm components
should continue to work unchanged.

## Possible implementation stages

1. Inventory existing pages and interactions. Define acceptance criteria for
   editors, the media browser, uploads, video, authentication, SSE, and the plugin
   context in particular.
2. Prepare shared business functions and the session adapter. A pilot with login,
   a layout, and a simple list/form page validates the architecture.
3. Build the independent Web Component host. The existing example at
   `crates/plugins/examples/vue-admin` must work without modification.
4. Add the server manifest and admin dispatcher. A Wasm example using Maud
   demonstrates a translated menu, list, detail page, and protected form action.
5. Migrate core pages incrementally. During the transition, each URL must belong
   to exactly one router; Vue fallback and Maud routes must not compete.
6. Once full feature parity is reached, remove Vue Router and Pinia from the core
   interface and update the release build, CSS scanning, asset embedding, and
   development instructions. The public `example/` remains an independent frontend.

## Acceptance criteria and open decisions

Verify login/verification, concurrent refresh, logout/revocation, CSRF, role
changes, direct page access, browser history, DE/EN, themes, media selection,
uploads, and SSE reconnection. Plugin checks additionally cover old manifests,
the Vue custom element, new server pages, namespace boundaries, XSS fragments,
redirects, oversized responses, and runtime failures. Core pages must remain
accessible even when a plugin fails.

Open decisions include session storage, the HTML allowlist and permitted UI
interactions for server plugins, CSS trust, the need for htmx/Alpine, and measured
build times for the isolated Maud preview. Potential benefits are shared Rust
types and less browser state; most of the effort lies in replacing interactive
Vue components and migrating authentication.
