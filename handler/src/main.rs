#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
// TODO: remove unwraps

mod media_controls;
mod tray;
mod filesystem;
mod config;
mod logger;
mod communication;

use std::sync::Arc;

use config::{GetError, Config};
// cargo is too dumb to realize that it's being used out of debug
#[allow(unused_imports)]
use daemonize::Daemonize;
use log::error;
use media_controls::Controls;

fn main() {
    let (config, config_err) = config::get_or_save_default();

    filesystem::create_file_structure(&config);

    // start logging to file
    logger::init(&config);

    // if the config originally failed to parse, notify the user
    if let Some(config_err) = config_err {
        error!("failed to parse config, got: {config_err}. Returned to defaults");
    }

    daemon(config);
}

fn daemon(config: Config) {
    // detatch (doesn't detatch in debug for.. debugging)
    #[cfg(not(debug_assertions))]
    Daemonize::new()
        .start().expect("Failed to start daemon");
    
    let config = Arc::new(config);

    // initialize gtk
    gtk::init().unwrap();

    // attach to media controls
    let controls = Controls::init(config.clone());
    let _watcher = filesystem::watch(controls.clone(), config.clone());

    // start system tray
    tray::create(controls, config).expect("to be able to create the system tray");

    // start gtk event loop
    gtk::main();

    // gtk has ended, cleanup
}

fn exit() {
    gtk::main_quit();
}
