use std::path::PathBuf;

use clap::Parser;
use inquire::{Password, PasswordDisplayMode, Select, Text};
use sqlx::{Pool, Postgres};

use crate::{
    db::{
        fields::{AuthRoleFields, Table},
        handles,
        models::{AuthRole, AuthUser, Role},
        queries::QueryObj,
    },
    utils::errors::NurError,
};

#[derive(Parser, Debug, Clone)]
#[clap(version,
    about = "nurCMS backend server",
    long_about = None)]
pub struct Args {
    #[clap(short, long, help = "Add user with role")]
    pub add_user: bool,

    #[clap(short, long, help = "Import Markdown files from given path")]
    pub import_markdown: Option<PathBuf>,

    #[clap(long, help = "Media folder for Markdown import")]
    pub import_media: Option<PathBuf>,

    #[clap(long, help = "Ignore files from import by its names")]
    pub ignore_files: Option<Vec<PathBuf>>,

    #[clap(short, long, help = "Listen on IP:PORT, like: 127.0.0.1:7777")]
    pub listen: Option<String>,

    #[clap(long, help = "Disabling two-factor authentication", hide = true)]
    pub disable_two_factor: bool,
}

pub async fn add_user(pool: &Pool<Postgres>) -> Result<(), NurError> {
    let query: QueryObj<AuthRoleFields> = QueryObj::default();

    let resp =
        handles::select_record::<AuthRoleFields, AuthRole>(pool, &Table::AuthRoles, query).await?;
    let role_list: Vec<Role> = resp.results.iter().map(|r| r.name.clone()).collect();

    let email = Text::new("Email:").prompt()?;
    let first_name = Text::new("First Name:").prompt()?;
    let last_name = Text::new("Last Name:").prompt()?;
    let username = Text::new("Username:").prompt()?;
    let password = Password::new("Password:")
        .with_display_mode(PasswordDisplayMode::Masked)
        .prompt()?;

    let role_name = Select::new("User role:", role_list).prompt()?;
    let role = resp.results.iter().find(|r| r.name == role_name).unwrap();
    let user = AuthUser::new(email, username, first_name, last_name, password, role.id);

    handles::insert_record::<AuthUser, i32>(pool, &Table::AuthUsers, &user).await?;

    Ok(())
}
