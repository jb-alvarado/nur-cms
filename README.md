# nur-cms

A simple and fast (headless) content management system built with Rust and Vue.js.

## Features

- **Fast & Efficient** - Rust backend with Axum web framework
- **Content Management** - Easy content editing with Markdown support
- **Media Management** - Resumable image and video uploads, responsive variants, configurable FFmpeg transcoding, and thumbnail management
- **Extended Markdown** - Comrak rendering with tables, footnotes, task lists, alerts, and additional formatting extensions
- **Internationalization** - Multi-language support
- **Modern UI** - Vue 3 + TypeScript frontend with Tailwind CSS and DaisyUI
- **RESTful API** - Clean API design
- **Flexible Content Output** - Delivers content as Markdown, rendered HTML, or a structured JSON AST
- **Public Entry Cache** - Configurable in-memory cache for public entry, list, and facet responses
- **WebAssembly Plugins** - Sandboxed backend plugins with API routes, trusted admin components, managed storage, host APIs, and isolated migrations
- **PostgreSQL Database** - Robust data storage with SQLx

For detailed setup instructions and development workflow, see the [Developer Documentation](docs/developer.md).

## Configuration

The application can be configured via:

- Versioned TOML configuration with per-instance config files
- Command-line arguments (see `cargo run -- --help`)

See the [configuration reference](docs/configuration.md) for available settings,
including the public entry cache.

See the [plugin documentation](docs/plugins.md) for building Wasmtime extensions with API routes,
role-based access, managed storage, plugin-local migrations, and admin components.

## Authentication

For two-factor authentication setup email credentials in the configuration. You can disable this with `--disable-two-factor`, which is useful when you want to seed the CMS from a script.

## Impressions

![Pages](/docs/screenshots/pages.png)

![Edit](/docs/screenshots/edit.png)

![Media](/docs/screenshots/media.png)

![Configuration](/docs/screenshots/configuration.png)
