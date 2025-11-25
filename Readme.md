# nur-cms

A simple and fast content management system built with Rust and Vue.js.

## Features

- **Fast & Efficient** - Rust backend with Axum web framework
- **Content Management** - Easy content editing with Markdown support
- **User Authentication** - Secure JWT-based authentication with role management
- **Media Management** - Image upload and processing (AVIF, WebP, PNG)
- **Internationalization** - Multi-language support (i18n)
- **Modern UI** - Vue 3 + TypeScript frontend with Tailwind CSS and DaisyUI
- **RESTful API** - Clean API design with Server-Sent Events (SSE) support
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
- `docker-compose.yml` for database settings
