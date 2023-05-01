use std::{path::{Path, PathBuf}, fs, io, env, process::Command, fmt::{Display, Debug}};

use aho_corasick::AhoCorasick;
use lazy_static::lazy_static;
use ron::ser::PrettyConfig;
use serde::{Serialize, Deserialize};

use log::*;

// TODO: accept null for mappings 

// HACK: make this better
fn get_home_dir() -> String {
    env::var("HOME").unwrap()
}

// HACK: make this better
fn get_username() -> String {
    get_home_dir()
        .replace("/home/", "")
}

lazy_static!(
    pub static ref REFERENCES: AhoCorasick = AhoCorasick::new(["{home_dir}", "{username}", "{wine_prefix}"]).unwrap();
);

fn replace(key: &str, config: &Config) -> String {
    match key {
        "{home_dir}" => get_home_dir(),
        "{username}" => get_username(),
        "{wine_prefix}" => config.commands.wine_prefix.get_checked(config),
        _ => panic!("tried to get the replacement for {key}, but it has no replacement"),
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)] // makes the reference act like a normal string when serialized
pub struct ReferencedString {
    template: String,
    #[serde(skip)]
    referred: Option<String>,
}

impl ReferencedString {
    pub fn new(template: String) -> Self {
        Self {
            template,
            referred: None,
        }
    }

    /// Gets the resolved value, panicking if it hasn't been resolved. A [Config] must
    /// be resolved after creation, so it is safe to use this method while in one.
    pub fn get(&self) -> &str {
        self.referred.as_ref().expect("tried to use an unresolved referenced string")
    }

    /// Gets the resolved value of the string without saving it
    pub fn get_checked(&self, config: &Config) -> String {
        self.referred.to_owned().unwrap_or_else(|| Self::resolve_str(&self.template, config))
    }

    /// Gets the resolved value of a reference and saves it
    pub fn resolve(&mut self, config: &Config) -> &str {
        self.referred.get_or_insert_with(|| Self::resolve_str(&self.template, config))
    }

    fn resolve_str(template: &str, config: &Config) -> String {
        let mut result = String::new();
        REFERENCES.replace_all_with(template, &mut result, |_, mat, dst| {
            dst.push_str(&replace(mat, config)); true
        });
        result
    }
}

impl From<&str> for ReferencedString {
    fn from(template: &str) -> Self {
        Self::new(template.to_string())
    }
}

impl Debug for ReferencedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.template)?;
        if let Some(referred) = &self.referred {
            write!(f, " ({})", referred)?
        }
        Ok(())
    }
}

impl Display for ReferencedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let string = self.referred.as_ref().unwrap_or(&self.template);
        write!(f, "{}", string)
    }
}

impl Mapping {
    pub fn resolve(&mut self, config: &mut Config) {
        self.from.resolve(config);
        self.to.resolve(config);
    }
}

impl Commands {
    pub fn resolve(&mut self, config: &mut Config) {
        self.wine_prefix.resolve(config);
    }
}

impl Config {
    pub fn resolve(mut self) -> Self {
        let mut cloned = self.clone();
        self.music_file_mapper.resolve(&mut cloned);
        self.temporary_file_mapper.resolve(&mut cloned);
        self.commands.resolve(&mut cloned);
        self
    }
}

/// Defines a simple replacement mapping
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Mapping {
    pub from: ReferencedString,
    pub to: ReferencedString,
}

impl Mapping {
    pub fn map(&self, string: &str) -> String {
        string.replace(self.from.get(), self.to.get())
    }
}

/// Info for running commands on MusicBee
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Commands {
    pub wine_command: String,
    pub wine_prefix: ReferencedString,
    pub musicbee_location: String,
}

impl Commands {
    pub fn run_command(&self, command: &str) {
        let mut cmd = Command::new(&self.wine_command);
         cmd.env("WINEPREFIX", self.wine_prefix.get())
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
#[derive(Clone, Serialize, Deserialize, Debug)]
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

// TODO: wrap in another type to make sure it gets resolved

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Config {
    pub commands: Commands,
    pub communication: Communication,
    pub music_file_mapper: Mapping,
    pub temporary_file_mapper: Mapping,
    pub detach_on_stop: bool,
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
        Self {
            communication: Communication::default(),
            commands: Commands::default(),
            music_file_mapper: Mapping {
                from: "C:/Users/{username}/Music".into(),
                to: "{home_dir}/Music".into(),
            },
            temporary_file_mapper: Mapping {
                from: "C:/".into(),
                to: "{wine_prefix}/drive_c/".into(),
            },
            detach_on_stop: true,
        }.resolve()
    }
}

impl Default for Commands {
    fn default() -> Self {
        Self {
            wine_command: "wine".to_string(),
            musicbee_location: r#"C:/Program Files/MusicBee/MusicBee.exe"#.to_string(),
            // TODO: this is an unreasonable default
            wine_prefix: "{home_dir}/Documents/executables/musicbee/.wine".into(),
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

fn config_folder() -> PathBuf {
    dirs::config_dir().unwrap()
        .join("musicbee-mediakeys")
}

fn config_path() -> PathBuf {
    config_folder().join("config.ron")
}

pub fn get_config() -> Config {
    let path = config_path();

    if path.exists() {
        let config = 
            ron::from_str::<Config>(
                &fs::read_to_string(&path)
                    .unwrap()
            );

        if let Ok(config) = config {
            return config.resolve();
        } // otherwise replace it with the default
    } 

    let config = Config::default();

    fs::create_dir_all(config_folder()).unwrap();
    fs::write(path,
        ron::ser::to_string_pretty(&config, PrettyConfig::new()).unwrap()
    ).unwrap();

    config
}
