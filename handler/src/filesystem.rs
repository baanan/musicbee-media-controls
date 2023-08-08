use std::{path::{Path, PathBuf}, ops::Deref, ffi::OsStr, fs::OpenOptions, time::Duration, sync::{Arc, Mutex}, io};

use anyhow::{Result, Context};
use log::*;
use souvlaki::*;
use notify::{Watcher, RecursiveMode, event::{Event, EventKind, ModifyKind}, RecommendedWatcher};
use thiserror::Error;
use url::Url;

use crate::{config::Config, listener::Listener};

pub const METADATA_FILE: &str = "metadata";
pub const PLAYBACK_FILE: &str = "playback";
pub const ACTION_FILE: &str = "action";
pub const PLUGIN_ACTIVATED_FILE: &str = "plugin-activated";
pub const VOLUME_FILE: &str = "volume";

pub fn watch(listener: Arc<Mutex<impl Listener + Send + 'static>>, config: Arc<Config>) -> notify::Result<RecommendedWatcher> {
    let communication_directory = config.communication.directory.clone();

    // start watching the filesystem
    let mut watcher = notify::recommended_watcher(move |event| {
        handle_event(event, &listener, &config);
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
    OpenOptions::new()
        .write(true).create(true).truncate(false)
        .open(config.get_comm_path(VOLUME_FILE))?;
    Ok(())
}

fn handle_event(event: notify::Result<Event>, listener: &Arc<Mutex<impl Listener>>, config: &Config) {
    let Ok(event) = event else { return };

    if let EventKind::Modify(ModifyKind::Data(_)) = event.kind {
        let file_names = event.paths.iter()
            .map(Deref::deref)
            .filter_map(Path::file_name)
            .filter_map(OsStr::to_str);

        for file_name in file_names {
            match file_name {
                METADATA_FILE => update_metadata(&mut *listener.lock().unwrap(), config)
                    .unwrap_or_else(|err| error!("failed to handle change in metadata: {err}")),
                PLAYBACK_FILE => update_playback(&mut *listener.lock().unwrap(), config)
                    .unwrap_or_else(|err| error!("failed to handle change in playback: {err}")),
                VOLUME_FILE => update_volume(&mut *listener.lock().unwrap(), config)
                    .unwrap_or_else(|err| error!("failed to handle change in volume: {err}")),
                PLUGIN_ACTIVATED_FILE => plugin_activation_changed(&mut *listener.lock().unwrap(), config)
                    .unwrap_or_else(|err| error!("failed to handle change in plugin activation: {err}")),
                _ => {},
            }
        }
    }
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

pub fn plugin_available(config: &Config) -> Result<Option<bool>> {
    let text = config.read_comm_file(PLUGIN_ACTIVATED_FILE)
        .context("failed to read plugin availability")?;

    // empty files are normal when they're being created
    if text.is_empty() { return Ok(None); }

    Ok(Some(text.parse().context("failed to parse plugin availability")?))
}

pub fn plugin_activation_changed(listener: &mut impl Listener, config: &Config) -> Result<()> {
    let Some(available) = plugin_available(config)? else { return Ok(()); };

    match (available, listener.attached()) {
        // exit if specified
        (false, _) if config.exit_with_plugin => {
            glib::idle_add(|| { crate::exit(); glib::Continue(false) }); },
        // attach/detach if needed
        (true, false) => listener.attach()?,
        (false, true) => listener.detach()?,
        _ => (),
    }

    Ok(())
}

pub fn update(listener: &mut impl Listener, config: &Config) -> Result<()> {
    update_metadata(listener, config)?;
    update_playback(listener, config)?;
    update_volume(listener, config)?;
    Ok(())
}

fn update_playback(listener: &mut impl Listener, config: &Config) -> Result<()> {
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
                return Err(MalformedFile::Playback(playback.trim().to_owned()))?;
            }
        };

        listener.set_playback(&playback)
            .context("failed to set the player's playback")?;
    } else {
        return Err(MalformedFile::Playback(playback.trim().to_owned()))?;
    }
    Ok(())
}

fn update_metadata(listener: &mut impl Listener, config: &Config) -> Result<()> {
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

        listener
            .set_metadata(&MediaMetadata {
                title: Some(title),
                album: Some(album),
                artist: Some(artist),
                cover_url: map_cover(cover_url, config, artist, title).as_deref(),
                duration: Some(duration),
            })
            .context("failed to set the player's metadata")?;
        Ok(())
    } else {
        Err(MalformedFile::Metadata(metadata))?
    }
}

fn update_volume(listener: &mut impl Listener, config: &Config) -> Result<()> {
    let volume = config.read_comm_file(VOLUME_FILE)
        .context("failed to read the volume file")?;

    // empty files are normal when they're being created
    if volume.is_empty() { return Ok(()); }

    let volume: f64 = volume.trim().parse()
        .map_err(|_| MalformedFile::Volume(volume))?;

    debug!("updating volume: {volume}");

    listener.set_volume(volume)
        .context("failed to set the player's volume")?;

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
