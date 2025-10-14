use clap::Parser;
use inquire::{Password, PasswordDisplayMode, Select, Text};
use sqlx::{Pool, Postgres};

use crate::{
    db::{handles, models::AuthUser},
    utils::errors::ServiceError,
};

#[derive(Parser, Debug, Clone)]
#[clap(version,
    about = "nurCMS backend server",
    long_about = None)]
pub struct Args {
    #[clap(short, long, help = "Add user with role")]
    pub add_user: bool,

    #[clap(short, long, help = "Listen on IP:PORT, like: 127.0.0.1:7777")]
    pub listen: Option<String>,

    #[clap(long, help = "Override logging level: trace, debug, info, warn, error")]
    pub log_level: Option<String>,

    #[clap(long, help = "Add timestamp to log line")]
    pub log_timestamp: bool,
}

pub async fn add_user(pool: &Pool<Postgres>) -> Result<(), ServiceError> {
    let roles = handles::select_auth_role(pool, None).await?;
    let role_list = roles.iter().map(|r| r.name.clone()).collect();

    let email = Text::new("Email:").prompt()?;
    let username = Text::new("Username:").prompt()?;
    let password = Password::new("Password:")
        .with_display_mode(PasswordDisplayMode::Masked)
        .prompt()?;

    let role_name = Select::new("User role:", role_list).prompt()?;
    let role = roles.iter().find(|r| r.name == role_name).unwrap();
    let user = AuthUser::new(email, username, password, role.id);

    handles::insert_auth_user(pool, user).await?;

    Ok(())
}
