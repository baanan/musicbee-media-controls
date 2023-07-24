use std::{path::{PathBuf, Path}, fs, io, env, process::Command, fmt::{Display, Debug}, time::Duration};

use aho_corasick::AhoCorasick;
use lazy_static::lazy_static;
use ron::ser::PrettyConfig;
use serde::{Serialize, Deserialize};
use thiserror::Error;
use anyhow::{Result, Context, Error};

use log::*;

// TODO: accept null for mappings 

// HACK: make this better
fn get_home_dir() -> String {
    env::var("HOME").expect("$HOME has the home directory")
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
        Self { template: template.to_owned() }
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


#[allow(clippy::doc_markdown)]
/// Info for running commands on MusicBee
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Commands<T> {
    pub wine_command: String,
    pub wine_prefix: T,
    pub musicbee_location: String,
}

impl Commands<ReferencedString> {
    pub fn run_command(&self, command: &str, arg: Option<String>) -> io::Result<()> {
        let mut cmd = Command::new(&self.wine_command);
         cmd.env("WINEPREFIX", self.wine_prefix.get())
            .arg(&self.musicbee_location)
            .arg(command);

        if let Some(ref arg) = arg {
            cmd.arg(arg);
        }

        trace!("Running command: \n    WINEPREFIX={} wine {} {} {}",
            self.wine_prefix,
            self.musicbee_location,
            command,
            arg.unwrap_or_default()
        );

        let _ = cmd.spawn()?.wait()?;

        trace!("Finished running command");
        Ok(())
    }

    pub fn run_simple_command(&self, command: &str) -> io::Result<()> {
        self.run_command(command, None)
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

pub type Config = Referenced<ReferencedString>;
type UnresolvedConfig = Referenced<UnresolvedReference>;

/// Global Config for the application
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Referenced<T> {
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

    pub fn run_command(&self, command: &str, arg: Option<String>) -> io::Result<()> {
        self.commands.run_command(command, arg)
    }

    pub fn run_simple_command(&self, command: &str) -> io::Result<()> {
        self.commands.run_simple_command(command)
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


#[derive(Debug, Error)]
pub enum GetError {
    #[error("config file not found")]
    NotFound,
}

pub const CONFIG_FILE: &str = "config.ron";

pub fn get_or_save_default(folder: &Path) -> (Config, Option<Error>) {
    match get(folder) {
        Ok(config) => (config, None),
        Err(err) => (
            save_default(folder).unwrap_or_default(),
            // don't error if the config isn't found
            (!err.is::<GetError>()).then_some(err)
        )
    }
}

pub fn get(folder: &Path) -> Result<Config> {
    let file = folder.join(CONFIG_FILE);
    if !file.exists() { return Err(GetError::NotFound.into()); }

    let contents = &fs::read_to_string(&file).context("failed to read config")?;
    ron::from_str::<Config>(contents).context("failed to parse config")
}

pub fn save_default(folder: &Path) -> Result<Config> {
    let file = folder.join(CONFIG_FILE);
    let config = Config::default();
    let serialized = ron::ser::to_string_pretty(&config, PrettyConfig::new())
        .context("failed to serialize default config")?;

    fs::create_dir_all(folder).and_then(|()|
        fs::write(file, serialized)
    )
        .context("failed to save default config")?;

    Ok(config)
}
