use std::{fs::File, sync::{Arc, Mutex}};

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
        .filter_module(env!("CARGO_CRATE_NAME"), LevelFilter::Trace) // log only from the current crate
        .init();
}

pub fn open(config: &Config) {
    let dir = &config.communication.directory;
    open::that(format!("{dir}/{FILE}")).unwrap();
}
