// #![allow(dead_code)]
// #![allow(unused_variables)]
// #![allow(unused_imports)]
#![allow(clippy::wildcard_imports)]

mod tray;
mod filesystem;
mod config;
mod logger;
mod communication;
mod cli;
mod daemon;
mod listener;
mod messages;

use std::time::Duration;

use clap::Parser;
use cli::{Cli, Commands};
// cargo is too dumb to realize that it's being used out of debug
#[allow(unused_imports)]
use daemonize::Daemonize;
use directories::ProjectDirs;
use futures::Future;
use log::*;
use anyhow::*;
use tokio::runtime::Runtime;

#[must_use]
fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("com.github", "baanan", "Musicbee Mediakeys")
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let (config, config_err) = config::get_or_save_default(&cli.config_path);

    filesystem::create_file_structure(&config)
        .context("failed to create the communication file structure")?;

    match cli.command {
        Commands::Run { run_config } => daemon::run(config, &run_config, config_err)?,
        Commands::End => 
            daemon::end(&config, true).context("failed to end daemon")?,
        Commands::ConfigFile { open: false } => 
            print!("{}", cli.config_file().display()),
        Commands::ConfigFile { open: true } => 
            open::that(cli.config_file()).context("failed to open config file")?,
    }

    Ok(())
}

// async is run later in daemon::run because daemonize breaks async
fn run_async(function: impl Future<Output = Result<()>>) -> Result<()> {
    let rt = Runtime::new().unwrap();
    let res = rt.block_on(function);
    rt.shutdown_timeout(Duration::from_secs(1));
    res
}
