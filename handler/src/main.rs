// #![allow(dead_code)]
// #![allow(unused_variables)]
// #![allow(unused_imports)]
#![allow(clippy::wildcard_imports)]

mod media_controls;
mod tray;
mod filesystem;
mod config;
mod logger;
mod communication;
mod cli;
mod daemon;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;
// cargo is too dumb to realize that it's being used out of debug
#[allow(unused_imports)]
use daemonize::Daemonize;
use directories::ProjectDirs;
use log::*;
use anyhow::*;

#[must_use]
fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("com.github", "baanan", "Musicbee Mediakeys")
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let (config, config_err) = config::get_or_save_default(&cli.config_path);

    filesystem::create_file_structure(&config)
        .context("failed to create the communication file structure")?;
    logger::init(&config)
        .context("failed to start logging")?;

    // if the config originally failed to parse, notify the user
    if let Some(config_err) = config_err {
        error!("failed to parse config, got: {config_err}. Returned to defaults");
    }

    match cli.command {
        Commands::Run { .. } => run(config, &cli.command)?,
        Commands::End => 
            daemon::end(&config, true).context("failed to end daemon")?,
        Commands::ConfigFile { open: false } => 
            print!("{}", cli.config_file().display()),
        Commands::ConfigFile { open: true } => 
            open::that(cli.config_file()).context("failed to open config file")?,
    }

    Ok(())
}

fn run(config: Config, command: &Commands) -> Result<()> {
    let Commands::Run { force, detach, tray, replace } = command else {
        panic!("expected command to be a Commands::Run");
    };

    if *replace { 
        daemon::end(&config, false).context("failed to replace previous daemon")?; 
    }

    if *force {
        daemon::create(config, *detach, *tray).context("failed to forcibly start daemon")?;
    } else {
        daemon::run(config, *detach, *tray).context("failed to start daemon")?;
    }

    Ok(())
}

fn exit() {
    gtk::main_quit();
}
