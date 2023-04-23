use std::fs::File;

use env_logger::Target;
use log::LevelFilter;

use crate::config::Config;

const FILE: &str = "handler.log";

pub fn init(config: &Config) {
    let dir = &config.communication.directory;
    let target = Box::new(File::create(format!("{dir}/{FILE}")).unwrap());

    env_logger::Builder::new()
        .target(Target::Pipe(target))
        .filter_level(LevelFilter::Warn)
        .filter_module("handler", LevelFilter::Trace) // INFO: this is the current crate, change if
                                                      // the name changes
        .init();
}

pub fn open(config: &Config) {
    let dir = &config.communication.directory;
    open::that(format!("{dir}/{FILE}")).unwrap();
}
