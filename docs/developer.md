# Developer Environment

## Requirements

1. Latest Rust version, best way to get it is [rustup](https://rustup.rs/)
2. Install **cargo-watch** with: `cargo install cargo-watch`
3. Node 22 or newer
4. Docker/Podman

## Getting Started

Clone the repository and `cd` into it:

```bash
git clone https://github.com/jb-alvarado/nur-cms.git && cd nur-cms
```

Then you need 3 terminal windows:

In the first terminal, run the developer database with:

```bash
podman compose up
```

Or if you're using Docker:

```bash
docker compose up
```

In the second terminal, run the Rust backend with:

```bash
cargo watch -C backend -x "run -- -l 127.0.0.1:8777"
```

In the third terminal, run the frontend with:

```bash
npm install

npm run dev
```

## Accessing the Application

- **Frontend**: <http://127.0.0.1:5757/admin/> (Vite development server)
- **Backend API**: <http://127.0.0.1:8777>
- **PostgreSQL**: 127.0.0.1:5442 (default credentials in `docker-compose.yml`)

## Project Structure

- `backend/` - Rust backend with Axum
- `example/` - Vue 3 example frontend with TypeScript
- `frontend/` - Vue 3 admin frontend with TypeScript
- `migrations/` - Database schema migrations
- `migrations_dev/` - Development database seed data
- `uploads/` - File upload directory

## Development Tips

- The backend automatically recompiles on file changes thanks to `cargo-watch`
- The frontend hot-reloads via Vite
- Database changes require stopping and restarting the database container
- Check `Cargo.toml` and `package.json` for available scripts and dependencies

## Troubleshooting

- If the backend fails to start, ensure the database is running and accessible
- If the frontend fails to build, try deleting `node_modules` and running `npm install` again
- Check that ports 5757, 8777, and 5442 are not already in use

## Building for Production

Build an optimized release binary (includes the frontend):

```bash
cargo build --release
```

The binary will be located at `target/release/nur-cms`.

Run the production binary:

```bash
./target/release/nur-cms -l 0.0.0.0:8777
```

The application (including the admin frontend) will be accessible at <http://0.0.0.0:8777/admin/>

You can also run the build script `./scripts/build.sh` to create __*.deb__ and __*.rpm__ packages.

### Deployment Considerations

**Backend:**

- Set appropriate environment variables (database connection, etc.)
- Ensure the `uploads/` directory is writable
- Consider using a process manager like `systemd` or `supervisord`
- Use a reverse proxy (nginx, caddy) for HTTPS and static file serving
- The frontend is embedded, so only the backend binary needs to be deployed

**Database:**

- Use a production PostgreSQL instance (not the Docker compose setup)
- Configure proper backup strategies
- Set secure passwords and restrict network access
- Apply regular security updates
