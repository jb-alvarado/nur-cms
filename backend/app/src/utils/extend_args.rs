use clap::Parser;

use core::utils::cmd_args::Args;

#[derive(Parser, Debug, Clone)]
pub struct AppArgs {
    #[command(flatten)]
    pub core: Args,

    #[clap(long, help = "Override logging level: trace, debug, info, warn, error")]
    pub log_level: Option<String>,

    #[clap(long, help = "Add timestamp to log line")]
    pub log_timestamp: bool,

    #[clap(long, help = "Serve uploads folder (not recommend)")]
    pub serve_static: bool,
}
