use std::{path::{Path, PathBuf}, ops::Deref, ffi::OsStr, fs::OpenOptions, time::Duration, sync::{Arc, Mutex}};

use log::{debug, error, warn};
use souvlaki::{MediaPlayback, MediaMetadata, MediaPosition};
use notify::{Watcher, RecursiveMode, Result, event::{Event, EventKind, ModifyKind}, RecommendedWatcher};
use url::Url;

use crate::{config::Config, media_controls::Controls};

pub const METADATA_FILE: &str = "metadata";
pub const PLAYBACK_FILE: &str = "playback";
pub const ACTION_FILE: &str = "action";
pub const PLUGIN_ACTIVATED_FILE: &str = "plugin-activated";

pub fn watch(controls: Arc<Mutex<Controls>>, config: Arc<Config>) -> RecommendedWatcher {
    let communication_directory = config.communication.directory.clone();

    // start watching the filesystem
    let mut watcher = notify::recommended_watcher(move |event| {
        handle_event(event, &controls, &config);
    }).unwrap();
    watcher.watch(Path::new(&communication_directory), RecursiveMode::NonRecursive)
        .unwrap();

    watcher
}

pub fn create_file_structure(config: &Config) {
    std::fs::create_dir_all(Path::new(&config.communication.directory)).unwrap();

    OpenOptions::new()
        .write(true).create(true).truncate(false)
        .open(config.get_comm_path(METADATA_FILE)).unwrap();
    OpenOptions::new()
        .write(true).create(true).truncate(false)
        .open(config.get_comm_path(PLAYBACK_FILE)).unwrap();
}

fn handle_event(event: Result<Event>, controls: &Arc<Mutex<Controls>>, config: &Config) {
    let Ok(event) = event else { return };

    if let EventKind::Modify(ModifyKind::Data(_)) = event.kind {
        let file_names = event.paths.iter()
            .map(Deref::deref)
            .filter_map(Path::file_name)
            .filter_map(OsStr::to_str);

        for file_name in file_names {
            match file_name {
                METADATA_FILE => update_metadata(&mut controls.lock().unwrap(), config),
                PLAYBACK_FILE => update_playback(&mut controls.lock().unwrap(), config),
                PLUGIN_ACTIVATED_FILE => plugin_activation_changed(&mut controls.lock().unwrap(), config),
                // TODO: plugin availablity watcher
                _ => {},
            }
        }
    }
}

pub fn plugin_available(config: &Config) -> bool {
    config.read_comm_file(PLUGIN_ACTIVATED_FILE).unwrap().trim() == "true"
}

pub fn plugin_activation_changed(controls: &mut Controls, config: &Config) {
    if !plugin_available(config) {
        if config.exit_with_plugin {
            glib::idle_add(|| { crate::exit(); glib::Continue(false) });
        } else {
            controls.detach();
        }
    } else if !controls.attached {
        controls.attach();
    }
}

pub fn update(controls: &mut Controls, config: &Config) {
    update_metadata(controls, config);
    update_playback(controls, config);
}

fn update_playback(controls: &mut Controls, config: &Config) {
    let playback = config.read_comm_file(PLAYBACK_FILE)
        .unwrap();

    // empty files are normal when they're being created
    if playback.is_empty() { return; }

    // split data by lines
    let lines: Vec<_> = playback.lines().collect();

    if let [ playback, progress ] = lines[..] {
        debug!("updating playback: {playback:?}");

        let progress = progress
            .parse::<u64>()
            .map(Duration::from_millis)
            .map(|pos| Some(MediaPosition(pos)))
            .unwrap();

        // sure, it may not be the most performant to match against a string, 
        // but it's good enough for now
        let playback = match playback.trim() {
            "stopped" => MediaPlayback::Stopped,
            "paused"  => MediaPlayback::Paused { progress },
            "playing" => MediaPlayback::Playing { progress },
            "loading" => return,
            _ => {
                error!("Playback value {} not found", playback.trim()); return;
            }
        };

        controls.set_playback(playback).unwrap();
    }   
}

fn update_metadata(controls: &mut Controls, config: &Config) {
    let metadata = config.read_comm_file(METADATA_FILE)
        .unwrap();

    // empty files are normal when they're being created
    if metadata.is_empty() { return; }

    // split data by lines
    let lines: Vec<_> = metadata.lines().collect();

    if let [ title, album, artist, cover_url, duration ] = lines[..] {
        debug!("updating metadata: {artist} - {title}");

        let duration = Duration::from_millis(duration.parse().unwrap());

        controls
            .set_metadata(MediaMetadata {
                title: Some(title),
                album: Some(album),
                artist: Some(artist),
                cover_url: map_cover(cover_url, config, artist, title).as_deref(),
                duration: Some(duration),
            })
            .unwrap();
    } else {
        warn!("Got malformed metadata from file, found:\n{metadata}");
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
        .map(|file| Url::from_file_path(file).unwrap().to_string())
}

// validates the cover and fixes it if possible
fn validate_cover(cover: &str, artist: &str, title: &str) -> Option<String> {
    let path = Path::new(cover);

    // if the file exists then it's all good
    if path.exists() { return Some(cover.to_owned()); }

    // since windows filenames don't care about capitalization,
    // musicbee sometimes gives a file with the wrong capitalization
    // this checks for it
    if let Some(capitalized) = change_cover_capitalization(path) {
        if capitalized.exists() {
            return Some(capitalized.into_os_string().into_string().unwrap())
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
