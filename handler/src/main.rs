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

// cargo is too dumb to realize that it's being used out of debug
#[allow(unused_imports)]
use daemonize::Daemonize;
use media_controls::Controls;

fn main() {
    // detatch (doesn't detatch in debug for debugging)
    #[cfg(not(debug_assertions))]
    Daemonize::new()
        .start().expect("Failed to start daemon");
    
    let config = Arc::new(config::get_config());

    filesystem::create_file_structure(&config);

    // start logging to file
    logger::init(&config);

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
