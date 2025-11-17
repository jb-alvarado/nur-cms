use axum::Router;
use clap::Parser;
use colored::Colorize;
use dotenvy::{dotenv, from_filename};
use tracing::{debug, error};

#[cfg(debug_assertions)]
use tower_http::services::ServeDir;

use nur_cms::{
    CONFIG, STORAGE,
    db::handles,
    init_db, router_entries,
    serve::routes::admin_routes,
    utils::{
        cmd_args::{Args, add_user},
        errors::ServiceError,
        logging::init_tracing,
    },
};

#[tokio::main]
async fn main() -> Result<(), ServiceError> {
    if dotenv().is_err() {
        from_filename(".env.example").ok();
    }

    let args = Args::parse();

    init_tracing(args.log_level, args.log_timestamp);

    let pool = init_db().await?;

    {
        let config = handles::select_configuration(&pool).await?;
        let mut cfg = CONFIG.write().await;
        *cfg = config;
    }

    if args.add_user {
        add_user(&pool).await?;

        return Ok(());
    }

    let (auth_routes, api_routes) = router_entries();

    let mut app = Router::new()
        .nest("/auth", auth_routes)
        .nest("/api", api_routes)
        .merge(admin_routes())
        .with_state(pool);

    #[cfg(debug_assertions)]
    {
        debug!("Dev mode: serving static files from {:?}", STORAGE.as_str());
        let uploads_service = ServeDir::new(&*STORAGE);
        app = app.nest_service("/uploads", uploads_service);
    }

    let listener =
        tokio::net::TcpListener::bind(args.listen.as_deref().unwrap_or("127.0.0.1:8777"))
            .await
            .map_err(|e| {
                error!("Failed to bind TCP listener: {e:?}");
                ServiceError::InternalServerError
            })?;

    if let Ok(addr) = listener.local_addr() {
        debug!("listening on {}", addr.to_string().yellow());
    } else {
        debug!("listening on bound address (local_addr unavailable)");
    }

    axum::serve(listener, app).await?;

    Ok(())
}
