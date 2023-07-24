use std::path::PathBuf;

use clap::{Parser, Subcommand, ArgAction};

use crate::config;

#[derive(Parser)]
#[command(author, version, about)]
pub struct Cli {
    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE", default_value_os_t = default_config_path())]
    pub config_path: PathBuf,

    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    pub fn config_file(&self) -> PathBuf {
        self.config_path.join(config::CONFIG_FILE)
    }
}

fn default_config_path() -> PathBuf {
    crate::project_dirs()
        .map_or_else(
            || dirs::home_dir().map(|home| home.join(".config/musicbeemediakeys"))
                // FIX: this isn't very graceful,
                // it would be nice to make the argument required if there wasn't any other way to
                // get a default, but I don't know how to do that
                .unwrap_or_default(), 
            |directories| directories.config_dir().to_owned()
        )
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run the daemon
    Run {
        /// Force the daemon to start without checking if one already exists
        #[arg(short, long)]
        force: bool,
        /// Don't detach the daemon from the terminal when created
        #[arg(short = 'd', long = "no-detach", default_value_t = true, action = ArgAction::SetFalse)]
        detach: bool,
        /// Don't create a tray item
        #[arg(short = 't', long = "no-tray", default_value_t = true, action = ArgAction::SetFalse)]
        tray: bool,
    },
    /// End the daemon
    End,
    /// Print the current config file
    ConfigFile {
        /// Open the file with the default application instead of printing it
        #[arg(short, long)]
        open: bool,
    },
}