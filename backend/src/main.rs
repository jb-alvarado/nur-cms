use axum::Router;
use clap::Parser;
use colored::Colorize;
use dotenvy::{dotenv, from_filename};
use tracing::{debug, error};

use nur_cms::{
    CONFIG,
    db::handles,
    init_db, router_entries,
    utils::{
        cmd_args::{Args, add_user},
        errors::ServiceError,
        logging::init_tracing,
    },
};

use nur_cms::serve::routes::admin_routes;

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

    let app = Router::new()
        .nest("/auth", auth_routes)
        .nest("/api", api_routes)
        .merge(admin_routes())
        .with_state(pool);

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
