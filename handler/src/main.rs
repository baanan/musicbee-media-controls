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

use std::sync::Arc;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;
// cargo is too dumb to realize that it's being used out of debug
#[allow(unused_imports)]
use daemonize::Daemonize;
use directories::ProjectDirs;
use log::*;
use media_controls::Controls;

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
        Commands::BecomeDaemon => daemon(config, &cli),
        Commands::ConfigFile { open: false } => print!("{}", cli.config_file().display()),
        Commands::ConfigFile { open: true } => open::that(cli.config_file()).expect("failed to open config file"),
    }
}

fn daemon(config: Config, cli: &Cli) {
    if cli.detach {
        Daemonize::new()
            .start().expect("failed to start daemon");
    }
    
    // share config
    let config = Arc::new(config);

    // initialize gtk
    gtk::init().unwrap();

    // attach to media controls
    let controls = Controls::init(config.clone())
        .expect("failed to initialize the media controls");
    let _watcher = filesystem::watch(controls.clone(), config.clone())
        .expect("failed to start to watch the filesystem");

    // start system tray
    tray::create(controls, config).expect("starting system tray");

    // start gtk event loop
    gtk::main();

    // gtk has ended, cleanup
}

fn exit() {
    gtk::main_quit();
}
