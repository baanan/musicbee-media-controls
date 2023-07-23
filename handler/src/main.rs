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

use config::ConfigGetError;
// cargo is too dumb to realize that it's being used out of debug
#[allow(unused_imports)]
use daemonize::Daemonize;
use log::error;
use media_controls::Controls;

fn main() {
    // detatch (doesn't detatch in debug for debugging)
    #[cfg(not(debug_assertions))]
    Daemonize::new()
        .start().expect("Failed to start daemon");
    
    let (config, config_err) = config::get_config_or_save_default();
    let config = Arc::new(config);

    filesystem::create_file_structure(&config);

    // start logging to file
    logger::init(&config);

    // if the config originally failed to parse, notify the user
    if let Some(config_parse_error) = config_err {
        error!("failed to parse config, got: {config_parse_error}. Returned to defaults");
    }

    // initialize gtk
    gtk::init().unwrap();

    // attach to media controls
    let controls = Controls::init(config.clone());
    let _watcher = filesystem::watch_filesystem(controls.clone(), config.clone());

    // start system tray
    tray::create(controls, config);

    // start gtk event loop
    gtk::main();

    // gtk has ended, cleanup
}

fn exit() {
    gtk::main_quit();
}
