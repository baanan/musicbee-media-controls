use std::{path::{Path, PathBuf}, ops::Deref, ffi::OsStr, fs::OpenOptions, time::Duration, sync::{Arc, Mutex}, io};

use anyhow::{Result, Context};
use log::*;
use souvlaki::*;
use notify::{Watcher, RecursiveMode, event::{Event, EventKind, ModifyKind}, RecommendedWatcher};
use thiserror::Error;
use url::Url;

use crate::{config::Config, media_controls::Controls};

pub const METADATA_FILE: &str = "metadata";
pub const PLAYBACK_FILE: &str = "playback";
pub const ACTION_FILE: &str = "action";
pub const PLUGIN_ACTIVATED_FILE: &str = "plugin-activated";

pub fn watch(controls: Arc<Mutex<Controls>>, config: Arc<Config>) -> notify::Result<RecommendedWatcher> {
    let communication_directory = config.communication.directory.clone();

    // start watching the filesystem
    let mut watcher = notify::recommended_watcher(move |event| {
        handle_event(event, &controls, &config);
    })?;
    watcher.watch(Path::new(&communication_directory), RecursiveMode::NonRecursive)?;

    Ok(watcher)
}

pub fn create_file_structure(config: &Config) -> io::Result<()> {
    std::fs::create_dir_all(Path::new(&config.communication.directory))?;

    OpenOptions::new()
        .write(true).create(true).truncate(false)
        .open(config.get_comm_path(METADATA_FILE))?;
    OpenOptions::new()
        .write(true).create(true).truncate(false)
        .open(config.get_comm_path(PLAYBACK_FILE))?;
    Ok(())
}

fn handle_event(event: notify::Result<Event>, controls: &Arc<Mutex<Controls>>, config: &Config) {
    let Ok(event) = event else { return };

    if let EventKind::Modify(ModifyKind::Data(_)) = event.kind {
        let file_names = event.paths.iter()
            .map(Deref::deref)
            .filter_map(Path::file_name)
            .filter_map(OsStr::to_str);

        for file_name in file_names {
            match file_name {
                METADATA_FILE => update_metadata(&mut controls.lock().unwrap(), config)
                    .unwrap_or_else(|err| error!("failed to handle change in metadata: {err}")),
                PLAYBACK_FILE => update_playback(&mut controls.lock().unwrap(), config)
                    .unwrap_or_else(|err| error!("failed to handle change in playback: {err}")),
                PLUGIN_ACTIVATED_FILE => plugin_activation_changed(&mut controls.lock().unwrap(), config)
                    .unwrap_or_else(|err| error!("failed to handle change in plugin activation: {err}")),
                // TODO: plugin availablity watcher
                _ => {},
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("playback value '{0}' not found")]
    MalformedPlayback(String),
    #[error("got malformed metadata: '{0}'")]
    MalformedMetadata(String),
}

pub fn plugin_available(config: &Config) -> Result<bool> {
    let text = config.read_comm_file(PLUGIN_ACTIVATED_FILE)?;
    Ok(text.trim() == "true")
}

pub fn plugin_activation_changed(controls: &mut Controls, config: &Config) -> Result<()> {
    let available = plugin_available(config)
        .context("failed to read the plugin availability file")?;

    match (available, controls.attached) {
        // exit if specified
        (false, _) if config.exit_with_plugin => {
            glib::idle_add(|| { crate::exit(); glib::Continue(false) }); },
        // attach/detach if needed
        (true, false) => controls.attach()?,
        (false, true) => controls.detach()?,
        _ => (),
    }

    Ok(())
}

pub fn update(controls: &mut Controls, config: &Config) -> Result<()> {
    update_metadata(controls, config)?;
    update_playback(controls, config)?;
    Ok(())
}

fn update_playback(controls: &mut Controls, config: &Config) -> Result<()> {
    let playback = config.read_comm_file(PLAYBACK_FILE)
        .context("failed to read the playback file")?;

    // empty files are normal when they're being created
    if playback.is_empty() { return Ok(()); }

    // split data by lines
    let lines: Vec<_> = playback.lines().collect();

    if let [ playback, progress ] = lines[..] {
        debug!("updating playback: {playback:?}");

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
                return Err(Error::MalformedPlayback(playback.trim().to_owned()))?;
            }
        };

        controls.set_playback(playback)
            .context("failed to set the player's playback")?;
    } else {
        return Err(Error::MalformedPlayback(playback.trim().to_owned()))?;
    }
    Ok(())
}

fn update_metadata(controls: &mut Controls, config: &Config) -> Result<()> {
    let metadata = config.read_comm_file(METADATA_FILE)
        .context("failed to read the metadata file")?;

    // empty files are normal when they're being created
    if metadata.is_empty() { return Ok(()); }

    // split data by lines
    let lines: Vec<_> = metadata.lines().collect();

    if let [ title, album, artist, cover_url, duration ] = lines[..] {
        debug!("updating metadata: {artist} - {title}");

        let duration = duration.parse()
            .map(Duration::from_millis)
            .context("failed to parse the song duration as a number")?;

        controls
            .set_metadata(MediaMetadata {
                title: Some(title),
                album: Some(album),
                artist: Some(artist),
                cover_url: map_cover(cover_url, config, artist, title).as_deref(),
                duration: Some(duration),
            })
            .context("failed to set the player's metadata")?;
        Ok(())
    } else {
        Err(Error::MalformedMetadata(metadata))?
    }
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
