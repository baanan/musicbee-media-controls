use std::fs::File;

use env_logger::{Builder, Target};
use log::LevelFilter;

use crate::config::Config;

const FILE: &str = "handler.log";

pub fn init(config: &Config) {
    let dir = &config.communication_directory;
    let target = Box::new(File::create(format!("{dir}/{FILE}")).unwrap());

    Builder::new()
        .target(Target::Pipe(target))
        .filter_level(LevelFilter::Info)
        .init();
}

pub fn open(config: &Config) {
    let dir = &config.communication_directory;
    open::that(format!("{dir}/{FILE}")).unwrap();
}
