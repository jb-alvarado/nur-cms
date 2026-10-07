# nur-cms

A modular content management system built with Rust and Vue.js.

Manage content and media in the admin panel, extend the CMS with plugins, and connect
your own website or application through the API.

## Features

- **Content Editing** - Custom content types, reusable content templates, and Markdown editing with preview, tables, footnotes, and task lists
- **Media Management** - Resumable uploads, automatic image variants, configurable video conversion and HLS playlist, thumbnails, and live conversion progress
- **Content Organization** - Categories, tags, authors, full-text search, and filters
- **Multilingual Content** - Manage content in multiple languages with a localized admin interface
- **Headless API** - Connect your own frontend and retrieve content as Markdown, rendered HTML, or structured JSON
- **Plugins** - Extend the CMS with WebAssembly plugins that add functionality, API routes, and admin pages
- **Access Control** - User roles and optional email two-factor authentication
- **Customization** - Configure branding, editor fields, default publishing status, and visible admin menu items

## Plugins and Examples

- [nur-blog](https://github.com/jb-alvarado/nur-blog) - A blog plugin for nur-cms
- [nur-notes](https://github.com/jb-alvarado/nur-notes) - An example frontend with a responsive card view, search, and filters for content from nur-cms

For detailed setup instructions and development workflow, see the [Developer Documentation](docs/developer.md).

## Configuration

The application can be configured via:

- Versioned TOML configuration with per-instance config files
- Command-line arguments (see `cargo run -- --help`)

See the [configuration reference](docs/configuration.md) for available settings.

See the [plugin documentation](docs/plugins.md) for installing and building plugins.

## Authentication

To enable email two-factor authentication, configure the SMTP settings. You can disable
it with `--disable-two-factor`, for example when seeding the CMS from a script.

## Impressions

Current admin interface with example content, in English and dark mode.

![Pages](/docs/screenshots/pages.png)

![Content editor with reusable blocks](/docs/screenshots/edit.png)

![Markdown editor with live preview](/docs/screenshots/preview.png)

![Media](/docs/screenshots/media.png)

![General Settings](/docs/screenshots/configuration.png)
![CMS settings](/docs/screenshots/cms-settings.png)

#### More screenshots

check: [screenshots](/docs/screenshots/)
