use std::{path::{Path, PathBuf}, fs, io, env, process::Command, fmt::Display};

use ron::ser::PrettyConfig;
use serde::{Serialize, Deserialize};

use log::*;

// TODO: accept null and {wineprefix}/... for mappings 

// both of these are dirty, but they work
fn get_home_dir() -> String {
    env::var("HOME").unwrap()
}

fn get_username() -> String {
    get_home_dir()
        .replace("/home/", "")
}

/// Defines a simple replacement mapping
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Mapping<V> {
    pub from: V,
    pub to: V,
}

impl Mapping<Unresolved> {
    pub fn resolve(self, config: &ConfigRes<Unresolved>) -> Mapping<Resolved> {
        Mapping {
            from: self.from.resolve(config),
            to: self.to.resolve(config),
        }
    }
}

impl Mapping<Resolved> {
    pub fn map(&self, string: &str) -> String {
        string.replace(self.from.get(), self.to.get())
    }
}

/// Info for running commands on MusicBee
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Commands<V> {
    pub wine_command: V,
    pub wine_prefix: V,
    pub musicbee_location: V,
}

impl Commands<Unresolved> {
    pub fn resolve(self, config: &ConfigRes<Unresolved>) -> Commands<Resolved> {
        Commands {
            wine_command: self.wine_command.resolve(config),
            wine_prefix: self.wine_prefix.resolve(config),
            musicbee_location: self.musicbee_location.resolve(config),
        }
    }
}

impl Commands<Resolved> {
    pub fn run_command(&self, command: &str) {
        let mut cmd = Command::new(self.wine_command.get());
         cmd.env("WINEPREFIX", self.wine_prefix.get())
            .arg(self.musicbee_location.get())
            .arg(command);

        debug!("Running command: \n    WINEPREFIX={} wine {} {}", self.wine_prefix, self.musicbee_location, command);

        let _ = cmd
            .spawn().unwrap()
            .wait().unwrap();

        trace!("Finished running command")
    }
}

/// Info for communication between the handler and the plugin
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Communication<V> {
    pub directory: V,
}

impl Communication<Unresolved> {
    pub fn resolve(self, config: &ConfigRes<Unresolved>) -> Communication<Resolved> {
        Communication {
            directory: self.directory.resolve(config),
        }
    }
}

impl Communication<Resolved> {
    pub fn get_comm_path(&self, name: &str) -> String {
        format!("{}/{name}", self.directory)
    }

    pub fn read_comm_file(&self, name: &str) -> io::Result<String> {
        fs::read_to_string(Path::new(&self.get_comm_path(name)))
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(transparent)]
struct Unresolved {
    template: String
}

impl Unresolved {
    pub fn resolve(self, config: &ConfigRes<Unresolved>) -> Resolved {
        let Unresolved { template } = self;
        let resolved = if template.contains("{") {
            template.replace("{wine_prefix}", &config.commands.wine_prefix.resolve(config).get())
        } else {
            template.clone()
        };

        Resolved { template, resolved }
    }
}

impl From<&str> for Unresolved {
    fn from(value: &str) -> Self {
        Self { template: value.to_string() }
    }
}

#[derive(Clone, Serialize, Debug)]
#[serde(transparent)]
struct Resolved {
    template: String,
    #[serde(skip)]
    resolved: String,
}

impl Resolved {
    pub fn get(&self) -> &str {
        &self.resolved
    }
}

impl Display for Resolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.resolved)
    }
}

pub type Config = ConfigRes<Resolved>;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ConfigRes<V> {
    pub commands: Commands<V>,
    pub communication: Communication<V>,
    pub music_file_mapper: Mapping<V>,
    pub temporary_file_mapper: Mapping<V>,
    pub detach_on_stop: bool,
}

impl ConfigRes<Unresolved> {
    pub fn resolve(self) -> ConfigRes<Resolved> {
        let cloned = self.clone();
        ConfigRes {
            commands: self.commands.resolve(&cloned),
            communication: self.communication.resolve(&cloned),
            music_file_mapper: self.music_file_mapper.resolve(&cloned),
            temporary_file_mapper: self.temporary_file_mapper.resolve(&cloned),
            detach_on_stop: self.detach_on_stop
        }
    }
}

impl ConfigRes<Resolved> {
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

impl Default for ConfigRes<Unresolved> {
    fn default() -> Self {
        let home_dir = get_home_dir();
        let username = get_username();

        let commands = Commands::default();
        let Commands { wine_prefix, .. } = &commands;

        Self {
            communication: Communication::default(),
            // NOTE: this might be unreasonable, but i'm not sure
            music_file_mapper: Mapping {
                from: "C:/Users/{username}/Music".into(),
                to: "{home_dir}/Music".into(),
            },
            temporary_file_mapper: Mapping {
                from: "C:/".into(),
                to: "{wine_prefix}/drive_c/".into(),
            },
            commands,
            detach_on_stop: true,
        }   
    }
}

impl Default for Commands<Unresolved> {
    fn default() -> Self {
        let home_dir = get_home_dir();

        Self {
            wine_command: "wine".into(),
            musicbee_location: r#"C:/Program Files/MusicBee/MusicBee.exe"#.into(),
            // TODO: this is an unreasonable default
            wine_prefix: "{home_dir}/Documents/executables/musicbee/.wine".into(),
        }
    }
}

impl Default for Communication<Unresolved> {
    fn default() -> Self {
        Self {
            directory: "/tmp/musicbee-mediakeys".into(),
        }
    }
}

fn config_folder() -> PathBuf {
    dirs::config_dir().unwrap()
        .join("musicbee-mediakeys")
}

fn config_path() -> PathBuf {
    config_folder().join("config.ron")
}

pub fn get_config() -> Config {
    let path = config_path();

    println!("{}", config_path().display());

    if path.exists() {
        if let Ok(config) = 
            ron::from_str::<ConfigRes<Unresolved>>(
                &fs::read_to_string(&path)
                    .unwrap()
            )
        { return config.resolve(); }
    } 

    let config = ConfigRes::<Unresolved>::default().resolve();

    fs::create_dir_all(config_folder()).unwrap();
    fs::write(path,
        ron::ser::to_string_pretty(&config, PrettyConfig::new()).unwrap()
    ).unwrap();

    config
}
