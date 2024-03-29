use std::{path::{Path, PathBuf}, ops::Deref, ffi::OsStr, fs::OpenOptions, time::Duration, io};

use anyhow::{Result, Context};
use async_trait::async_trait;
use log::*;
use souvlaki::*;
use notify::{Watcher, RecursiveMode, event::{Event, EventKind, ModifyKind}, RecommendedWatcher};
use thiserror::Error;
use url::Url;

use crate::{config::Config, messages::{MessageSender, Command}, listener::Listener};

pub const METADATA_FILE: &str = "metadata";
pub const PLAYBACK_FILE: &str = "playback";
pub const ACTION_FILE: &str = "action";
pub const PLUGIN_ACTIVATED_FILE: &str = "plugin-activated";
pub const VOLUME_FILE: &str = "volume";

pub fn watch(message_sender: MessageSender, config: &Config) -> Result<RecommendedWatcher> {
    let communication_directory = config.communication.directory.clone();

    let mut watcher = notify::recommended_watcher(move |event| handle_event(event, &message_sender))?;
    watcher.watch(Path::new(&communication_directory), RecursiveMode::NonRecursive)?;

    Ok(watcher)
}

fn handle_event(event: notify::Result<Event>, sender: &MessageSender) {
    let Ok(event) = event else { return };

    // FIX: remove repeats
    if let EventKind::Modify(ModifyKind::Data(_)) = event.kind {
        let file_names = event.paths.iter()
            .map(Deref::deref)
            .filter_map(Path::file_name)
            .filter_map(OsStr::to_str);

        for file_name in file_names {
            match file_name {
                METADATA_FILE => sender.update_metadata(),
                PLAYBACK_FILE => sender.update_playback(),
                VOLUME_FILE => sender.update_volume(),
                PLUGIN_ACTIVATED_FILE => sender.update_plugin_activation(),
                _ => {},
            }
        }
    }
}

pub struct Filesystem { sender: MessageSender }

impl Filesystem {
    pub const fn new(sender: MessageSender) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl Listener for Filesystem {
    async fn handle(&mut self, command: Command, config: &Config) -> Result<()> {
        let Self { sender } = self;
        match command {
            Command::Update => 
                update(sender, config).await.context("failed to update handlers")?,
            Command::UpdateMetadata => 
                update_metadata(sender, config).await.context("failed to update metadata")?,
            Command::UpdatePlayback => 
                update_playback(sender, config).await.context("failed to update playback")?,
            Command::UpdateVolume => 
                update_volume(sender, config).await.context("failed to update volume")?,
            Command::UpdatePluginActivation => 
                plugin_activation_changed(sender, config).await.context("failed to update plugin activation")?,
            _ => (),
        }
        Ok(())
    }

    fn name(&self) -> &'static str { "filesystem" }
}

pub fn create_file_structure(config: &Config) -> io::Result<()> {
    std::fs::create_dir_all(Path::new(&config.communication.directory))?;

    OpenOptions::new()
        .write(true).create(true).truncate(false)
        .open(config.get_comm_path(METADATA_FILE))?;
    OpenOptions::new()
        .write(true).create(true).truncate(false)
        .open(config.get_comm_path(PLAYBACK_FILE))?;
    OpenOptions::new()
        .write(true).create(true).truncate(false)
        .open(config.get_comm_path(VOLUME_FILE))?;
    OpenOptions::new()
        .write(true).create(true).truncate(false)
        .open(config.get_comm_path(PLUGIN_ACTIVATED_FILE))?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum MalformedFile {
    #[error("playback value '{0}' not found")]
    Playback(String),
    #[error("got malformed metadata: '{0}'")]
    Metadata(String),
    #[error("got malformed volume: '{0}'")]
    Volume(String),
}

pub async fn plugin_available(config: &Config) -> Result<Option<bool>> {
    let text = config.read_comm_file(PLUGIN_ACTIVATED_FILE).await
        .context("failed to read plugin availability")?;

    // empty files are normal when they're being created
    if text.is_empty() { return Ok(None); }

    Ok(Some(text.parse().context("failed to parse plugin availability")?))
}

async fn plugin_activation_changed(send: &MessageSender, config: &Config) -> Result<()> {
    if let Some(activated) = plugin_available(config).await? {
        send.plugin_activated(activated);
    }
    Ok(())
}

async fn update(send: &MessageSender, config: &Config) -> Result<()> {
    let (metadata, playback, volume) = futures::join!(
        update_metadata(send, config),
        update_playback(send, config),
        update_volume(send, config),
    );
    metadata.context("failed to update metadata")?;
    playback.context("failed to update playback")?;
    volume.context("failed to update volume")?;
    Ok(())
}

async fn update_playback(send: &MessageSender, config: &Config) -> Result<()> {
    let playback = config.read_comm_file(PLAYBACK_FILE).await
        .context("failed to read the playback file")?;

    // empty files are normal when they're being created
    if playback.is_empty() { return Ok(()); }

    // split data by lines
    let lines: Vec<_> = playback.lines().collect();

    if let [ playback, progress ] = lines[..] {
        let progress = progress.parse()
            .map(Duration::from_millis)
            .map(|p| Some(MediaPosition(p)))
            .context("failed to parse the playback progress as a number")?;

        // sure, it may not be the most performant to match against a string, 
        // but it's good enough for now
        let playback = match playback.trim() {
            "stopped" => MediaPlayback::Stopped,
            "paused"  => MediaPlayback::Paused { progress },
            "playing" => MediaPlayback::Playing { progress },
            "loading" => return Ok(()),
            _ => {
                return Err(MalformedFile::Playback(playback.trim().to_owned()))?;
            }
        };

        send.playback(playback);
    } else {
        return Err(MalformedFile::Playback(playback.trim().to_owned()))?;
    }
    Ok(())
}

async fn update_metadata(send: &MessageSender, config: &Config) -> Result<()> {
    let metadata = config.read_comm_file(METADATA_FILE).await
        .context("failed to read the metadata file")?;

    // empty files are normal when they're being created
    if metadata.is_empty() { return Ok(()); }

    // split data by lines
    let lines: Vec<_> = metadata.lines().collect();

    if let [ title, album, artist, cover_url, duration ] = lines[..] {
        let duration = duration.parse()
            .map(Duration::from_millis)
            .context("failed to parse the song duration as a number")?;

        send
            .metadata(MediaMetadata {
                title: Some(title),
                album: Some(album),
                artist: Some(artist),
                cover_url: map_cover(cover_url, config, artist, title).as_deref(),
                duration: Some(duration),
            });
        Ok(())
    } else {
        Err(MalformedFile::Metadata(metadata))?
    }
}

async fn update_volume(send: &MessageSender, config: &Config) -> Result<()> {
    let volume = config.read_comm_file(VOLUME_FILE).await
        .context("failed to read the volume file")?;

    // empty files are normal when they're being created
    if volume.is_empty() { return Ok(()); }

    let volume: f64 = volume.trim().parse()
        .map_err(|_| MalformedFile::Volume(volume))?;

    send.volume(volume);

    Ok(())
}

fn map_cover(
    cover: &str, config: &Config,
    artist: &str, title: &str
) -> Option<String> {
    // if the cover is empty then just return no cover
    if cover.is_empty() {
        warn!("Got no cover for track: {artist} - {title}");
        return None;
    }

    let cover = &config.map_filename(cover);

    validate_cover(cover, artist, title)
        .map(|file| Url::from_file_path(file)
            .expect("file url already passes file_exists_at")
            .to_string())
}

fn file_exists_at(path: &Path) -> bool { path.is_absolute() && path.is_file() }

// validates the cover and fixes it if possible
fn validate_cover(cover: &str, artist: &str, title: &str) -> Option<String> {
    let path = Path::new(cover);

    // if the file exists then it's all good
    if file_exists_at(path) { return Some(cover.to_owned()); }

    // since windows filenames don't care about capitalization,
    // musicbee sometimes gives a file with the wrong capitalization.
    // this checks for it
    if let Some(capitalized) = change_cover_capitalization(path) {
        if file_exists_at(&capitalized) {
            return Some(capitalized.into_os_string().into_string()
                .expect("path came from a string, so it must be valid unicode"))
        }
    }

    error!("Got cover for track: {artist} - {title} at {cover}, but no file was found there");

    None
}

fn change_cover_capitalization(path: &Path) -> Option<PathBuf> {
    let Some(filename) = path.file_stem().and_then(OsStr::to_str) else { return None };

    // only the folder covers get messed up
    // and it's annoying to capitalize the first letter of a string
    // so might as well just hardcode it
    let replacement = match filename {
        "folder" => "Folder",
        "Folder" => "folder",
        "cover"  => "Cover",
        "Cover"  => "cover",
        _ => return None,
    };

    let mut new = replacement.to_owned();

    if let Some(extension) = path.extension().and_then(OsStr::to_str) {
        new = new + "." + extension;
    }

    Some(path.with_file_name(new))
}
