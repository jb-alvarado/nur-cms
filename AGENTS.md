# Project Instructions

nur-cms is a headless CMS with a Rust/Axum and SQLx/PostgreSQL backend plus a
Vue 3/TypeScript admin UI using Vite, Tailwind CSS, DaisyUI, Pinia, and SSE.
Production builds embed the frontend in the Rust binary.

## Structure

- `backend/app/`: executable, CLI, middleware, logging, and static UI serving
- `backend/core/`: API, auth, database, uploads, mail, SSE, and shared logic
- `migrations/`: PostgreSQL migrations
- `frontend/`: admin UI served below `/admin/`
- `example/`: public example frontend
- `assets/`, `docs/`, `scripts/`, `debian/`: configuration and project tooling

## Working rules

- Inspect relevant code and search with `rg` before editing. Reuse established
  abstractions and preserve unrelated changes in a dirty worktree.
- Keep frontend, API/database, and deployment responsibilities separate. Prefer
  small functions, early returns, narrow exports, and propagated errors.
- Treat requests, content, uploads, paths, MIME types, API/SSE payloads, and
  client identities as untrusted. Preserve authorization and never expose or
  log credentials, tokens, verification codes, or internal storage paths.
- Keep migrations, SQLx models and queries, serialization, validation, API
  responses, and generated TypeScript declarations consistent. Add a migration
  instead of rewriting existing history unless explicitly requested.
- Keep async Rust non-blocking; do not hold locks across awaits. Avoid
  `unwrap`/`expect` in production paths without a documented invariant.
- Use Vue Composition API and typed TypeScript. Do not introduce `any`; prefer
  generated declarations, type guards, or `unknown` at external boundaries.
- Preserve `/auth`, `/api`, `/sse`, `/uploads`, the Vite `/admin/` base path,
  the auth/refresh flow, and the established alert and responsive DaisyUI
  patterns.
- Add English and German translations for user-facing text and focused tests
  for behavior, validation boundaries, errors, and authorization.
- `npm run lint` applies fixes; review its changes afterward.

## Verification

Run checks relevant to the change and report anything that could not run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace
cargo test --workspace
npm run lint
npm run type-check
npm run build
```
