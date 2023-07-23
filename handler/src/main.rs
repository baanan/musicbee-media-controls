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
// cargo is too dumb to realize that it's being used out of debug
#[allow(unused_imports)]
use daemonize::Daemonize;
use directories::ProjectDirs;
use log::*;

#[must_use]
fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("com.github", "baanan", "Musicbee Mediakeys")
}

fn main() {
    let cli = Cli::parse();

    let (config, config_err) = config::get_or_save_default(&cli.config_path);

    filesystem::create_file_structure(&config)
        .expect("failed to create the communication file structure");
    logger::init(&config)
        .expect("failed to start logging");

    // if the config originally failed to parse, notify the user
    if let Some(config_err) = config_err {
        error!("failed to parse config, got: {config_err}. Returned to defaults");
    }

    match cli.command {
        Commands::Run { force: false, detach, tray } => 
            daemon::run(config, detach, tray).expect("failed to start daemon"),
        Commands::Run { force: true, detach, tray } => 
            daemon::create(config, detach, tray),
        Commands::End => 
            daemon::end(&config).expect("failed to end daemon"),
        Commands::ConfigFile { open: false } => 
            print!("{}", cli.config_file().display()),
        Commands::ConfigFile { open: true } => 
            open::that(cli.config_file()).expect("failed to open config file"),
    }
}

fn exit() {
    gtk::main_quit();
}
