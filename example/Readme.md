# NUR CMS Example Frontend

This is an example frontend project that demonstrates how to use the NUR CMS backend API. It serves as a reference implementation and starting point for building custom frontends with NUR CMS.

## Overview

The example frontend is a minimal Vue 3 application that showcases:
- How to connect to the NUR CMS backend API
- Basic project structure for a Vue-based frontend
- API proxy configuration for development
- Tailwind CSS integration

## Getting Started

### Prerequisites

- Node.js (v20.19.0 or v22.12.0+)
- NUR CMS backend running on `http://127.0.0.1:8777`

### Installation

Dependencies are installed at the root level of the NUR CMS project:

```bash
cd /path/to/nur-cms
npm install
```

### Development

Start the example frontend development server:

```bash
npm run dev:example
```

The application will be available at `http://127.0.0.1:5758`

### Building for Production

Build the example frontend:

```bash
npm run build:example
```

The compiled files will be in the `example/dist` directory.

## Project Structure

```
example/
├── src/
│   ├── App.vue          # Main application component
│   ├── main.ts          # Application entry point
│   └── assets/
│       └── main.css     # Tailwind CSS imports
├── index.html           # HTML template
├── env.d.ts             # TypeScript environment declarations
├── tsconfig.json        # TypeScript configuration
└── Readme.md           # This file
```

## Configuration

The example frontend is configured via `vite.example.config.ts` in the project root:

- **Port**: 5758 (different from main admin frontend on 5757)
- **Base URL**: `/`
- **API Proxies**: All API requests are proxied to the backend at `http://127.0.0.1:8777`

### API Endpoints Proxied

- `/api/*` - Main API endpoints
- `/auth/*` - Authentication endpoints
- `/sse/*` - Server-Sent Events
- `/uploads/*` - Media files

## Using the Backend API

### Authentication

```typescript
// Login example
const login = async (username: string, password: string) => {
  const response = await fetch('/auth/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password })
  })
  const { access, refresh } = await response.json()
  return { access, refresh }
}
```

### Fetching Content

```typescript
// Get published entries
const getEntries = async (accessToken: string) => {
  const response = await fetch('/api/content/entries?status=published&output_type=html', {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  })
  const data = await response.json()
  return data.results
}

// Get a single entry
const getEntry = async (typeSlug: string, slug: string) => {
  const response = await fetch(`/api/content/entries/${typeSlug}/${slug}?output_type=html`)
  const entry = await response.json()
  return entry
}
```

### Creating Content

```typescript
// Create a new entry (requires authentication)
const createEntry = async (accessToken: string, entryData: any) => {
  const response = await fetch('/api/content/entries', {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify(entryData)
  })
  return await response.json()
}
```

## Example Use Cases

This example frontend can be extended to demonstrate:

1. **Blog/News Site**: Fetch and display published articles
2. **Portfolio**: Showcase projects and work samples
3. **Documentation**: Present structured documentation with categories
4. **Multi-language Site**: Use the locales system for i18n content
5. **Custom Admin**: Build a specialized content management interface

## Key Features to Explore

### Content Output Formats

The API supports three output formats:

```typescript
// Markdown (raw)
fetch('/api/content/entries?output_type=markdown')

// HTML (pre-rendered)
fetch('/api/content/entries?output_type=html')

// AST (JSON structure for custom rendering)
fetch('/api/content/entries?output_type=ast')
```

### Filtering & Pagination

```typescript
const params = new URLSearchParams({
  page: '1',
  page_size: '10',
  status: 'published',
  category_slug: 'technology',
  order_by: 'published_at',
  order_dir: 'desc'
})

fetch(`/api/content/entries?${params}`)
```

### Media Files

```typescript
// Fetch media metadata
const getMedia = async () => {
  const response = await fetch('/api/media')
  return await response.json()
}

// Display images
// <img src="/uploads/2025/12/image.avif" alt="..." />
```

## Technology Stack

- **Vue 3**: Progressive JavaScript framework
- **TypeScript**: Type-safe development
- **Vite**: Fast build tool and dev server
- **Tailwind CSS**: Utility-first CSS framework

## Differences from Main Frontend

The main admin frontend (`/frontend`) provides:
- Full admin interface for content management
- User authentication and authorization UI
- Media upload and management
- SSE-based real-time updates
- Multi-language admin interface

This example frontend is intentionally minimal to:
- Serve as a clean starting point
- Demonstrate basic API integration
- Be easily customizable for your needs

## API Documentation

For complete API documentation, see:
- Backend API endpoints: See [docs/developer.md](../docs/developer.md)
- Route implementations: `/backend/src/api/routes/`
- Frontend API utilities: `/frontend/src/api/`

## Extending This Example

To build your custom frontend:

1. **Add Vue Router** for navigation between pages
2. **Add Pinia** for state management (like the main frontend)
3. **Create API service modules** to organize API calls
4. **Implement authentication flow** with token refresh
5. **Add components** for displaying content, forms, etc.
6. **Style with Tailwind** or your preferred CSS framework

## Running Both Frontends

You can run both the admin and example frontends simultaneously:

```bash
# Terminal 1: database
docker compose up

# Terminal 2: Backend
cargo run

# Terminal 3: Admin Frontend
npm run dev

# Terminal 4: Example Frontend
npm run dev:example
```

- Admin Frontend: http://127.0.0.1:5757
- Example Frontend: http://127.0.0.1:5758
- Backend API: http://127.0.0.1:8777

## Troubleshooting

### Port Already in Use

If port 5758 is already in use, modify `vite.example.config.ts`:

```typescript
server: {
  port: 5759, // Change to any available port
  // ...
}
```

### Backend Connection Issues

Ensure the backend is running on port 8777:

```bash
cargo run
```

Check the proxy configuration in `vite.example.config.ts` matches your backend URL.

### TypeScript Errors

Make sure TypeScript dependencies are installed:

```bash
npm install
```

## Contributing

This example is part of the NUR CMS project. Contributions that improve the example or documentation are welcome!

## License

Same as the main NUR CMS project.
