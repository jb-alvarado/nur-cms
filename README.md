# nur-cms

A simple and fast (headless) content management system built with Rust and Vue.js.

**This project is open source, but it is not currently a "community project", so there is no support, and feature requests are not welcome. However, minor pull requests may be accepted under certain circumstances.**

## Features

- **Fast & Efficient** - Rust backend with Axum web framework
- **Content Management** - Easy content editing with Markdown support
- **Media Management** - Image upload and processing (AVIF, WebP, JPG, PNG)
- **Internationalization** - Multi-language support
- **Modern UI** - Vue 3 + TypeScript frontend with Tailwind CSS and DaisyUI
- **RESTful API** - Clean API design with Server-Sent Events (SSE)
- **Flexible Content Output** - Delivers content in 3 formats: Markdown, HTML, and AST (JSON structure)
- **PostgreSQL Database** - Robust data storage with SQLx

For detailed setup instructions and development workflow, see the [Developer Documentation](docs/developer.md).

## Configuration

The application can be configured via:

- Environment variables (`.env` file)
- Command-line arguments (see `cargo run -- --help`)

## Authentication

For two-factor authentication setup email credentials in the configuration. You can disable this with `--disable-two-factor`, which is useful when you want to seed the CMS from a script.

## Impressions

![Pages](/docs/screenshots/pages.png)

![Edit](/docs/screenshots/edit.png)

![Media](/docs/screenshots/media.png)

![Configuration](/docs/screenshots/configuration.png)
