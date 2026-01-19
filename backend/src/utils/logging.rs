use chrono::Local;
use tracing_subscriber::{
    EnvFilter, Layer,
    fmt::{self, format::Writer, time::FormatTime},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

struct ChronoLocalTimer;

impl FormatTime for ChronoLocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", Local::now().format("[%Y-%m-%d %H:%M:%S%.6f]"))
    }
}

pub fn init_tracing(level: Option<String>, timestamp: bool) {
    let filter = match level {
        Some(l) => EnvFilter::new(format!(
            "sqlx=warn,tower_http=info,{l}={}",
            env!("CARGO_CRATE_NAME")
        )),
        None => EnvFilter::new(format!(
            "sqlx=warn,tower_http=info,{}=debug",
            env!("CARGO_CRATE_NAME")
        )),
    };

    let fmt_layer = if timestamp {
        fmt::layer()
            .with_timer(ChronoLocalTimer)
            .with_target(false)
            .with_level(true)
            .with_ansi(true)
            .with_thread_ids(false)
            .with_thread_names(false)
            .boxed()
    } else {
        fmt::layer()
            .compact()
            .without_time()
            .with_target(false)
            .with_level(true)
            .with_ansi(true)
            .with_thread_ids(false)
            .with_thread_names(false)
            .boxed()
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();
}
