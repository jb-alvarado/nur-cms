# nur-cms

A simple and fast content management system built with Rust and Vue.js.

**Note:** This project is in an early stage of development. Therefore, errors may occur.

**This project is open source, but it is not currently a "community project", so there is no support, and feature requests are not welcome. However, minor pull requests may be accepted under certain circumstances.**

## Features

- **Fast & Efficient** - Rust backend with Axum web framework
- **Content Management** - Easy content editing with Markdown support
- **User Authentication** - Secure JWT-based authentication with role management
- **Media Management** - Image upload and processing (AVIF, WebP, PNG)
- **Internationalization** - Multi-language support (i18n)
- **Modern UI** - Vue 3 + TypeScript frontend with Tailwind CSS and DaisyUI
- **RESTful API** - Clean API design with Server-Sent Events (SSE) support
- **Flexible Content Output** - API delivers content in three formats: Markdown, HTML, and AST (JSON structure)
- **PostgreSQL Database** - Robust data storage with SQLx

## Technology Stack

### Backend

- **Rust** with Axum web framework
- PostgreSQL database with SQLx
- JWT authentication (argon2 password hashing)
- Image processing (AVIF, WebP, PNG)
- Markdown parsing

### Frontend

- **Vue 3** with TypeScript
- Vue Router for navigation
- Pinia for state management
- Tailwind CSS + DaisyUI for styling
- Vite for fast development and building
- Vue i18n for internationalization

For detailed setup instructions and development workflow, see the [Developer Documentation](docs/developer.md).

## Configuration

The application can be configured via:

- Environment variables (`.env` file)
- Command-line arguments (see `cargo run -- --help`)

## Authentication

For two-factor authentication setup email credentials in the configuration.

## Impressions

![Pages](/docs/screenshots/pages.png)

![Media](/docs/screenshots/media.png)

![Configuration](/docs/screenshots/configuration.png)
