#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
// TODO: remove unwraps

mod media_controls;
mod tray;
mod filesystem;
mod config;

use std::env;

use config::{Mapping, Config};
use daemonize::Daemonize;

fn main() {
    // detatch (doesn't detatch in debug for debugging)
    #[cfg(not(debug_assertions))]
    Daemonize::new()
        .start().expect("Failed to start daemon");
    
    let config = config::get_config();

    // initialize gtk
    gtk::init().unwrap();

    // start system tray
    tray::create();

    // attach to media controls
    let media_controls = media_controls::create(config.clone());
    let _watcher = filesystem::watch_filesystem(media_controls, config);

    // start gtk event loop
    gtk::main();

    // gtk has ended, cleanup
}
