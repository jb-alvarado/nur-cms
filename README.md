# nur-cms

A simple and fast (headless) content management system built with Rust and Vue.js.

## Features

- **Fast & Efficient** - Rust backend with Axum web framework
- **Content Management** - Easy content editing with Markdown support
- **Media Management** - Image upload and processing (AVIF, WebP, JPG, PNG)
- **Internationalization** - Multi-language support
- **Modern UI** - Vue 3 + TypeScript frontend with Tailwind CSS and DaisyUI
- **RESTful API** - Clean API design
- **Flexible Content Output** - Delivers content in 3 formats: Markdown, HTML, and AST (JSON structure)
- **Public Entry Cache** - Configurable in-memory cache for public entry, list, and facet responses
- **WebAssembly Plugins** - Sandboxed Wasmtime plugins with API routes and isolated migrations
- **PostgreSQL Database** - Robust data storage with SQLx

For detailed setup instructions and development workflow, see the [Developer Documentation](docs/developer.md).

## Configuration

The application can be configured via:

- Environment variables (`.env` file)
- Command-line arguments (see `cargo run -- --help`)

See the [configuration reference](docs/configuration.md) for available environment variables,
including the public entry cache.

See the [plugin documentation](docs/plugins.md) for building Wasmtime extensions with API routes,
role-based access, plugin-local migrations, and future admin menu metadata.

## Authentication

For two-factor authentication setup email credentials in the configuration. You can disable this with `--disable-two-factor`, which is useful when you want to seed the CMS from a script.

## Impressions

![Pages](/docs/screenshots/pages.png)

![Edit](/docs/screenshots/edit.png)

![Media](/docs/screenshots/media.png)

![Configuration](/docs/screenshots/configuration.png)
