use std::{path::Path, ops::Deref, ffi::OsStr, fs::{self, OpenOptions, File}, time::Duration, os, thread};

use souvlaki::{MediaControls, MediaPlayback, MediaMetadata};
use notify::{Watcher, RecursiveMode, Result, event::*, RecommendedWatcher};
use const_format::concatcp;

use crate::config::Config;

const METADATA_FILE: &str = "metadata";
const PLAYBACK_FILE: &str = "playback";

pub fn watch_filesystem(mut controls: MediaControls, config: Config) -> RecommendedWatcher {
    let communication_directory = config.communication_directory.clone();

    create_file_structure(&config);

    // get initial info
    update(&mut controls, &config);

    // start watching the filesystem
    let mut watcher = notify::recommended_watcher(move |event| {
        handle_event(event, &mut controls, &config)
    })
        .unwrap();

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

fn handle_event(event: Result<Event>, controls: &mut MediaControls, config: &Config) {
    let Ok(event) = event else { return };

    if let EventKind::Modify(ModifyKind::Data(_)) = event.kind {
        let file_names = event.paths.iter()
            .map(Deref::deref)
            .filter_map(Path::file_name)
            .filter_map(OsStr::to_str);

        for file_name in file_names {
            match file_name {
                METADATA_FILE => update_metadata(controls, config),
                PLAYBACK_FILE => update_playback(controls, config),
                // TODO: plugin availablity watcher
                _ => {},
            }
        }
    }
}

fn update(controls: &mut MediaControls, config: &Config) {
    update_metadata(controls, config);
    update_playback(controls, config);
}

fn update_playback(controls: &mut MediaControls, config: &Config) {
    let playback = get_playback(config);

    // TODO: don't panic when playback not found
    if let Some(playback) = playback {
        controls.set_playback(playback)
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
        "" => None,
        _ => {
            panic!("playback value {playback} not found");
        }
    }
}

fn update_metadata(controls: &mut MediaControls, config: &Config) {
    let metadata = config.read_comm_file(METADATA_FILE)
        .unwrap();

    let lines: Vec<_> = metadata.lines().collect();

    if let [ title, album, artist, cover_url, duration ] = lines[..] {
        let duration = Duration::from_millis(duration.parse().unwrap());

        controls.set_metadata(MediaMetadata {
            title: Some(title),
            album: Some(album),
            artist: Some(artist),
            cover_url: map_cover(cover_url, config).as_deref(),
            duration: Some(duration),
        })
            .unwrap();
    }
}

fn map_cover(cover: &str, config: &Config) -> Option<String> {
    // if the cover is empty then just return no cover
    // TODO: investigate empty cover art
    if cover.is_empty() { return None }

    Some("file://".to_string() + &config.map_filename(cover))
}
