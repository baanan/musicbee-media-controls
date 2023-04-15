use std::{path::Path, fs, io, env, process::Command};

use log::*;

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

#[derive(Clone)]
pub struct Commands {
    pub wine_command: String,
    pub wine_prefix: String,
    pub musicbee_location: String,
}

impl Default for Commands {
    fn default() -> Self {
        let home_dir = env::var("HOME").unwrap();

        // TODO: this is an unreasonable default
        let wine_prefix = format!("{home_dir}/Documents/executables/musicbee/.wine");

        Self {
            wine_command: "wine".to_string(),
            musicbee_location: r#"C:/Program Files/MusicBee/MusicBee.exe"#.to_string(),
            wine_prefix,
        }
    }
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

#[derive(Clone)]
pub struct Config {
    // TODO: extract wine_prefix and musicbee_location to another struct
    // and add some methods to run commands easier
    pub commands: Commands,
    pub communication_directory: String,
    pub music_file_mapper: Mapping,
    pub temporary_file_mapper: Mapping,
}

impl Config {
    pub fn get_comm_path(&self, name: &str) -> String {
        format!("{}/{name}", self.communication_directory)
    }

    pub fn read_comm_file(&self, name: &str) -> io::Result<String> {
        fs::read_to_string(Path::new(&self.get_comm_path(name)))
    }

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
}

impl Default for Config {
    fn default() -> Self {
        // this is dirty, but it works
        let home_dir = env::var("HOME").unwrap();
        let username = home_dir.replace("/home/", "");

        let commands = Commands::default();
        let Commands { wine_prefix, .. } = &commands;

        Self {
            // wine_location: format!(r#"/home/{username}/.var/app/com.usebottles.bottles/data/bottles/runners/wine-ge-proton7-41/bin/wine"#),
            communication_directory: "/tmp/musicbee-mediakeys".to_string(),
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
        }   
    }
}

pub fn get_config() -> Config {
    Config::default()
}
