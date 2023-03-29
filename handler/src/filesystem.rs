use std::{path::Path, ops::Deref, ffi::OsStr, fs::{OpenOptions}, time::Duration, sync::{Arc, Mutex}};

use log::*;
use souvlaki::{MediaPlayback, MediaMetadata};
use notify::{Watcher, RecursiveMode, Result, event::*, RecommendedWatcher};

use crate::{config::Config, media_controls::Controls};

const METADATA_FILE: &str = "metadata";
const PLAYBACK_FILE: &str = "playback";

pub fn watch_filesystem(controls: Arc<Mutex<Controls>>, config: Config) -> RecommendedWatcher {
    let communication_directory = config.communication_directory.clone();

    create_file_structure(&config);

    // get initial info
    update(controls.clone(), &config);

    // start watching the filesystem
    let mut watcher = notify::recommended_watcher(move |event| {
        handle_event(event, controls.clone(), &config)
    }).unwrap();
    watcher.watch(Path::new(&communication_directory), RecursiveMode::NonRecursive)
        .unwrap();

    watcher
}

fn create_file_structure(config: &Config) {
    std::fs::create_dir_all(Path::new(&config.communication_directory)).unwrap();

    OpenOptions::new()
        .write(true).create(true).truncate(false)
        .open(config.get_comm_path(METADATA_FILE)).unwrap();
    OpenOptions::new()
        .write(true).create(true).truncate(false)
        .open(config.get_comm_path(PLAYBACK_FILE)).unwrap();
}

fn handle_event(event: Result<Event>, mut controls: Arc<Mutex<Controls>>, config: &Config) {
    let Ok(event) = event else { return };

    if let EventKind::Modify(ModifyKind::Data(_)) = event.kind {
        let file_names = event.paths.iter()
            .map(Deref::deref)
            .filter_map(Path::file_name)
            .filter_map(OsStr::to_str);

        for file_name in file_names {
            match file_name {
                METADATA_FILE => update_metadata(&mut controls, config),
                PLAYBACK_FILE => update_playback(&mut controls, config),
                // TODO: plugin availablity watcher
                _ => {},
            }
        }
    }
}

pub fn update(mut controls: Arc<Mutex<Controls>>, config: &Config) {
    update_metadata(&mut controls, config);
    update_playback(&mut controls, config);
}

fn update_playback(controls: &mut Arc<Mutex<Controls>>, config: &Config) {
    let playback = get_playback(config);

    // TODO: don't panic when playback not found
    if let Some(playback) = playback {
        info!("updating playback");

        controls.lock().unwrap()
            .set_playback(playback)
            .unwrap();
    }
}

fn get_playback(config: &Config) -> Option<MediaPlayback> {
    let playback = config.read_comm_file(PLAYBACK_FILE)
        .unwrap();

    // sure, it may not be the most performant to match against a string, 
    // but it's good enough for now
    match playback.trim() {
        "stopped" => Some(MediaPlayback::Stopped),
        "paused" => Some(MediaPlayback::Paused { progress: None }),
        "playing" => Some(MediaPlayback::Playing { progress: None }),
        "" => None, // empty files are normal when they're being created
        _ => {
            error!("Playback value {playback} not found"); None
        }
    }
}

fn update_metadata(controls: &mut Arc<Mutex<Controls>>, config: &Config) {
    let metadata = config.read_comm_file(METADATA_FILE)
        .unwrap();

    // empty files are normal when they're being created
    if metadata.is_empty() { return; }

    // split data by lines
    let lines: Vec<_> = metadata.lines().collect();

    if let [ title, album, artist, cover_url, duration ] = lines[..] {
        info!("updating metadata");

        let duration = Duration::from_millis(duration.parse().unwrap());

        controls.lock().unwrap()
            .set_metadata(MediaMetadata {
                title: Some(title),
                album: Some(album),
                artist: Some(artist),
                cover_url: map_cover(cover_url, config, artist, title).as_deref(),
                duration: Some(duration),
            })
            .unwrap();
    } else {
        warn!("Got malformed metadata from file, found:\n{metadata}")
    }
}

fn map_cover(
    cover: &str, config: &Config,
    artist: &str, title: &str
) -> Option<String> {
    // if the cover is empty then just return no cover
    // TODO: investigate empty cover art
    // it seems to be a problem with weird filenames
    if cover.is_empty() {
        error!("Got no cover for track: {artist} - {title}");
        return None;
    }

    let cover = &config.map_filename(cover);

    validate_cover(cover, artist, title).map(
        |file| format!("file://{file}")
    )
}

// validates the cover and fixes it if possible
fn validate_cover(cover: &str, artist: &str, title: &str) -> Option<String> {
    // if the file exists then it's all good
    if Path::new(cover).exists() { return Some(cover.to_owned()); }

    // sometimes musicbee messes up and forgets a capital for some unknowable reason
    if cover.ends_with("folder.jpg") {
        let capitalized = cover.replace("folder.jpg", "Folder.jpg");
        if Path::new(&capitalized).exists() { return Some(capitalized) }
    }

    error!("Got cover for track: {artist} - {title} at {cover}, but no file was found there");

    None
}
