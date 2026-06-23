# NUR CMS Example Frontend

This is a small public Vue frontend for NUR CMS. It demonstrates how to read published content from the backend without admin authentication.

## What it shows

- Listing published `article` entries from `/api/content/entries`
- Fetching one article from `/api/content/entries/{type}/{slug}`
- Using the real query parameters used by the backend: `fields`, `type`, `locale`, `limit`, `offset`, `ordering`
- Rendering content nodes when the backend returns AST, HTML, or Markdown
- Displaying media and generated image variants from `/uploads`

## Prerequisites

- Node.js `^20.19.0` or `>=22.12.0`
- NUR CMS backend running on `http://127.0.0.1:8777`
- At least one published content entry of type `article`

## Run it

Install dependencies from the repository root:

```bash
npm install
```

Start the backend and then run the example frontend:

```bash
npm run dev:example
```

The example is served at `http://127.0.0.1:5758`.

## Build it

```bash
npm run build:example
```

The compiled files are written to `example/dist`.

## Configuration

The example uses `vite.example.config.ts` in the repository root.

Relevant defaults:

- frontend port: `5758`
- backend proxy target: `http://127.0.0.1:8777`
- content type: `article`
- locale: `en`

You can override the content type and locale at build/dev time:

```bash
VITE_NUR_ARTICLE_TYPE=article VITE_NUR_LOCALE=de npm run dev:example
```

## API notes

Public requests are treated as guest requests. The backend automatically restricts entries to `status=published`.

The example list request is equivalent to:

```text
GET /api/content/entries
  ?type=article
  &locale=en
  &fields=id,title,slug,media,created_at,category.name,category.slug,tags,node.text
  &limit=6
  &offset=0
  &ordering=-created_at
  &blocks_limit=1
  &character_limit=240
```

The response shape is:

```ts
{
  count: number
  next: string | null
  previous: string | null
  results: ContentEntrySerializer[]
}
```

Single article request:

```text
GET /api/content/entries/article/my-slug
  ?locale=en
  &fields=id,title,slug,media,created_at,category.name,category.slug,tags,author.first_name,author.last_name,node.id,node.order_index,node.text,node.media,node.data
```

`output_type` can only be overridden by authenticated admin/author requests. Public requests use the backend configuration. The example therefore handles all supported node outputs:

- `node.ast`
- `node.html`
- `node.text`

## Project structure

```text
example/
├── src/
│   ├── api/content.ts          # Small API wrapper for public content requests
│   ├── components/AstRender.vue
│   ├── components/HomeArticle.vue
│   ├── views/HomeView.vue
│   ├── views/ArticleView.vue
│   └── utils/helper.ts
├── index.html
├── tsconfig.json
└── Readme.md
```

## Running both frontends

```bash
# Terminal 1: database
docker compose up

# Terminal 2: backend
cargo run

# Terminal 3: admin frontend
npm run dev

# Terminal 4: example frontend
npm run dev:example
```

- Admin frontend: `http://127.0.0.1:5757`
- Example frontend: `http://127.0.0.1:5758`
- Backend API: `http://127.0.0.1:8777`
