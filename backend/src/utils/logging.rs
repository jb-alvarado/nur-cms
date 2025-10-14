use chrono::Local;
use tracing_subscriber::{
    EnvFilter, Layer,
    fmt::{self, format::Writer, time::FormatTime},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

use crate::ARGS;
struct ChronoLocalTimer;

impl FormatTime for ChronoLocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", Local::now().format("%Y-%m-%d %H:%M:%S%.6f"))
    }
}

pub fn init_tracing() {
    let filter = if let Some(ref level) = ARGS.log_level {
        EnvFilter::new(format!("sqlx=warn,{}={}", env!("CARGO_CRATE_NAME"), level))
    } else {
        EnvFilter::new(format!("sqlx=warn,{}=debug", env!("CARGO_CRATE_NAME")))
    };

    let fmt_layer = if ARGS.log_timestamp {
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
