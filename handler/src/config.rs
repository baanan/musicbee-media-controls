use std::{path::PathBuf, fs, io, env, process::Command, fmt::{Display, Debug}, time::Duration};

use aho_corasick::AhoCorasick;
use lazy_static::lazy_static;
use ron::{ser::PrettyConfig, error::SpannedError};
use serde::{Serialize, Deserialize};

use log::*;
use souvlaki::SeekDirection;

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
    // searches for multiple things at a time
    pub static ref REFERENCES: AhoCorasick = AhoCorasick::new(["{home_dir}", "{username}", "{wine_prefix}"]).unwrap();
);

fn replace(key: &str, config: &UnresolvedConfig) -> String {
    match key {
        "{home_dir}" => get_home_dir(),
        "{username}" => get_username(),
        "{wine_prefix}" => config.commands.wine_prefix.get_recursive(config),
        _ => panic!("tried to get the replacement for {key}, but it has no replacement"),
    }
}

#[derive(Clone, Deserialize)]
#[serde(transparent)] // makes the reference act like a normal string when serialized
pub struct UnresolvedReference {
    template: String,
}

impl UnresolvedReference {
    /// Gets the resolved value of a reference and saves it
    pub fn resolve(self, config: &UnresolvedConfig) -> ReferencedString {
        ReferencedString {
            referred: Self::resolve_str(&self.template, config),
            template: self.template,
        }
    }

    fn resolve_str(template: &str, config: &UnresolvedConfig) -> String {
        let mut result = String::new();
        REFERENCES.replace_all_with(template, &mut result, |_, mat, dst| {
            dst.push_str(&replace(mat, config)); true
        });
        result
    }

    fn get_recursive(&self, config: &UnresolvedConfig) -> String {
        Self::resolve_str(&self.template, config)
    }
}

impl From<&str> for UnresolvedReference {
    fn from(template: &str) -> Self {
        UnresolvedReference { template: template.to_owned() }
    }
}

#[derive(Clone, Serialize)]
#[serde(transparent)] // makes the reference act like a normal string when serialized
pub struct ReferencedString {
    template: String,
    #[serde(skip)]
    referred: String,
}

impl ReferencedString {
    pub fn get(&self) -> &str {
        &self.referred
    }
}

// resolvers

impl Mapping<UnresolvedReference> {
    pub fn resolve(self, config: &UnresolvedConfig) -> Mapping<ReferencedString> {
        Mapping {
            from: self.from.resolve(config),
            to: self.to.resolve(config)
        }
    }
}

impl Commands<UnresolvedReference> {
    pub fn resolve(self, config: &UnresolvedConfig) -> Commands<ReferencedString> {
        Commands {
            wine_prefix: self.wine_prefix.resolve(config),
            wine_command: self.wine_command,
            musicbee_location: self.musicbee_location,
        }
    }
}

impl UnresolvedConfig {
    pub fn resolve(self) -> Config {
        // the config has to be cloned to make sure the values don't change while it's being read
        let cloned = self.clone();
        Config {
            music_file_mapper: self.music_file_mapper.resolve(&cloned),
            temporary_file_mapper: self.temporary_file_mapper.resolve(&cloned),
            commands: self.commands.resolve(&cloned),
            communication: self.communication,
            detach_on_stop: self.detach_on_stop,
            exit_with_plugin: self.exit_with_plugin,
            seek_amount: self.seek_amount,
        }
    }
}

impl From<UnresolvedConfig> for Config {
    fn from(value: UnresolvedConfig) -> Self {
        value.resolve()
    }
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de> 
    {
        UnresolvedConfig::deserialize(deserializer).map(UnresolvedConfig::resolve)
    }
}

// ReferencedString trait implementations

impl Debug for ReferencedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.template, self.referred)?;
        Ok(())
    }
}

impl Display for ReferencedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.referred)
    }
}


/// Defines a simple replacement mapping
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Mapping<T> {
    pub from: T,
    pub to: T,
}

impl Mapping<ReferencedString> {
    pub fn map(&self, string: &str) -> String {
        string.replace(self.from.get(), self.to.get())
    }
}


/// Info for running commands on MusicBee
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Commands<T> {
    pub wine_command: String,
    pub wine_prefix: T,
    pub musicbee_location: String,
}

impl Commands<ReferencedString> {
    pub fn run_command(&self, command: &str) {
        let mut cmd = Command::new(&self.wine_command);
         cmd.env("WINEPREFIX", self.wine_prefix.get())
            .arg(&self.musicbee_location)
            .arg(command);

        trace!("Running command: \n    WINEPREFIX={} wine {} {}", self.wine_prefix, self.musicbee_location, command);

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
    pub fn get_comm_path(&self, name: &str) -> PathBuf {
        PathBuf::from(format!("{}/{name}", self.directory))
    }

    pub fn read_comm_file(&self, name: &str) -> io::Result<String> {
        fs::read_to_string(self.get_comm_path(name))
    }

    pub fn write_comm_file(&self, name: &str, contents: &str) -> io::Result<()> {
        fs::write(self.get_comm_path(name), contents)
    }
}


// TODO: wrap in another type to make sure it gets resolved

pub type Config = ReferencedConfig<ReferencedString>;
type UnresolvedConfig = ReferencedConfig<UnresolvedReference>;

/// Global Config for the application
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ReferencedConfig<T> {
    pub commands: Commands<T>,
    pub communication: Communication,
    pub music_file_mapper: Mapping<T>,
    pub temporary_file_mapper: Mapping<T>,
    pub detach_on_stop: bool,
    pub exit_with_plugin: bool,
    pub seek_amount: Duration,
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

    pub fn get_comm_path(&self, name: &str) -> PathBuf {
        self.communication.get_comm_path(name)
    }

    pub fn read_comm_file(&self, name: &str) -> io::Result<String> {
        self.communication.read_comm_file(name)
    }

    pub fn write_comm_file(&self, name: &str, contents: &str) -> io::Result<()> {
        self.communication.write_comm_file(name, contents)
    }
}


impl Default for Config {
    fn default() -> Self {
        UnresolvedConfig {
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
            exit_with_plugin: true,
            seek_amount: Duration::from_secs(5),
        }
            .resolve()
    }
}

impl Default for Commands<UnresolvedReference> {
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


// FIX: use that other dirs crate
fn config_folder() -> PathBuf {
    dirs::config_dir().unwrap()
        .join("musicbee-mediakeys")
}

fn config_path() -> PathBuf {
    config_folder().join("config.ron")
}

pub fn get_config() -> (Config, Option<SpannedError>) {
    let path = config_path();

    // deserialize path

    let err = 
        if path.exists() {
            let contents = &fs::read_to_string(&path).unwrap();
            match ron::from_str::<Config>(contents) {
                Ok(config) => return (config, None),
                // return the error and use a default config since the logger isn't initialized yet
                Err(err) => Some(err),
            }
        } else {
            None
        };

    // fallback

    let config = Config::default();
    let serialized = ron::ser::to_string_pretty(&config, PrettyConfig::new()).unwrap();

    fs::create_dir_all(config_folder()).unwrap();
    fs::write(path, serialized).unwrap();

    (config, err)
}
