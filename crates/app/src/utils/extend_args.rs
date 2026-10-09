use std::path::PathBuf;

use clap::{Args as ClapArgs, Parser, Subcommand};

use nur_core::utils::cmd_args::Args;

#[derive(Parser, Debug, Clone)]
pub struct AppArgs {
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help = "Use this configuration file"
    )]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<AppCommand>,

    #[command(flatten)]
    pub core: Args,

    #[clap(long, help = "Override logging level: trace, debug, info, warn, error")]
    pub log_level: Option<String>,

    #[clap(long, help = "Add timestamp to log line")]
    pub log_timestamp: bool,

    #[clap(long, help = "Serve uploads folder (not recommend)")]
    pub serve_static: bool,
}

#[derive(Clone, Debug, Subcommand)]
pub enum AppCommand {
    /// Create, validate, or migrate a configuration file.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Clone, Debug, Subcommand)]
pub enum ConfigCommand {
    /// Create a new configuration file.
    Create(CreateConfigArgs),
    /// Validate a configuration file without starting the server.
    Check(ConfigPathArgs),
    /// Migrate a configuration file to the current format.
    Migrate(ConfigPathArgs),
}

#[derive(Clone, Debug, ClapArgs)]
pub struct ConfigPathArgs {
    /// File to check or migrate; otherwise use the normal search order.
    pub path: Option<PathBuf>,
}

#[derive(Clone, Debug, ClapArgs)]
pub struct CreateConfigArgs {
    /// Destination of the new configuration.
    pub path: PathBuf,

    /// Import known settings from this dotenv file.
    #[arg(long, value_name = "PATH", conflicts_with_all = ["from_environment", "no_env"])]
    pub from_env: Option<PathBuf>,

    /// Import known settings from the current process environment.
    #[arg(long, conflicts_with = "no_env")]
    pub from_environment: bool,

    /// Do not offer to import a .env file from the current directory.
    #[arg(long)]
    pub no_env: bool,

    /// Replace an existing file after creating a backup.
    #[arg(long)]
    pub force: bool,
}
