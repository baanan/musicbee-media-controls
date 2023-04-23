use std::{path::Path, fs, io, env, process::Command};

use log::*;

// both of these are dirty, but they work
fn get_home_dir() -> String {
    env::var("HOME").unwrap()
}

fn get_username() -> String {
    get_home_dir()
        .replace("/home/", "")
}

/// Defines a simple replacement mapping
#[derive(Clone)]
pub struct Mapping {
    pub from: String,
    pub to: String,
}

impl Mapping {
    pub fn map(&self, string: &str) -> String {
        string.replace(&self.from, &self.to)
    }
}

/// Info for running commands on MusicBee
#[derive(Clone)]
pub struct Commands {
    pub wine_command: String,
    pub wine_prefix: String,
    pub musicbee_location: String,
}

impl Commands {
    pub fn run_command(&self, command: &str) {
        let mut cmd = Command::new(&self.wine_command);
         cmd.env("WINEPREFIX", &self.wine_prefix)
            .arg(&self.musicbee_location)
            .arg(command);

        debug!("Running command: \n    WINEPREFIX={} wine {} {}", self.wine_prefix, self.musicbee_location, command);

        let _ = cmd
            .spawn().unwrap()
            .wait().unwrap();

        trace!("Finished running command")
    }
}

/// Info for communication between the handler and the plugin
#[derive(Clone)]
pub struct Communication {
    pub directory: String,
}

impl Communication {
    pub fn get_comm_path(&self, name: &str) -> String {
        format!("{}/{name}", self.directory)
    }

    pub fn read_comm_file(&self, name: &str) -> io::Result<String> {
        fs::read_to_string(Path::new(&self.get_comm_path(name)))
    }
}

#[derive(Clone)]
pub struct Config {
    pub commands: Commands,
    pub communication: Communication,
    pub music_file_mapper: Mapping,
    pub temporary_file_mapper: Mapping,
    // pub detach_on_stop: bool,
}

impl Config {
    pub fn map_filename(&self, name: &str) -> String {
        // we don't like \ here
        let name = name.replace('\\', "/");

        // TODO: better check
        if name.contains("Temp") {
            self.temporary_file_mapper.map(&name)
        } else {
            self.music_file_mapper.map(&name)
        }
    }

    pub fn run_command(&self, command: &str) {
        self.commands.run_command(command)
    }

    pub fn get_comm_path(&self, name: &str) -> String {
        self.communication.get_comm_path(name)
    }

    pub fn read_comm_file(&self, name: &str) -> io::Result<String> {
        self.communication.read_comm_file(name)
    }
}

impl Default for Config {
    fn default() -> Self {
        let home_dir = get_home_dir();
        let username = get_username();

        let commands = Commands::default();
        let Commands { wine_prefix, .. } = &commands;

        Self {
            communication: Communication::default(),
            // NOTE: this might be unreasonable, but i'm not sure
            music_file_mapper: Mapping {
                from: format!("C:/Users/{username}/Music"),
                to: format!("{home_dir}/Music"),
            },
            temporary_file_mapper: Mapping {
                from: "C:/".to_string(),
                to: format!("{wine_prefix}/drive_c/")
            },
            commands,
            // detach_on_stop: true,
        }   
    }
}

impl Default for Commands {
    fn default() -> Self {
        let home_dir = get_home_dir();

        // TODO: this is an unreasonable default
        let wine_prefix = format!("{home_dir}/Documents/executables/musicbee/.wine");

        Self {
            wine_command: "wine".to_string(),
            musicbee_location: r#"C:/Program Files/MusicBee/MusicBee.exe"#.to_string(),
            wine_prefix,
        }
    }
}

impl Default for Communication {
    fn default() -> Self {
        Self {
            directory: "/tmp/musicbee-mediakeys".to_string(),
        }
    }
}

pub fn get_config() -> Config {
    Config::default()
}
