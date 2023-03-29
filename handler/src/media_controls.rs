use std::{process::Command, sync::{Arc, Mutex}};

use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, PlatformConfig};

use crate::config::Config;

// spoilers
// pub type Controls = Arc<Mutex<Option<MediaControls>>>;

// pub fn initialize() -> Controls {
//     Arc::new(Mutex::new(None))
// }

pub fn create(config: Config) -> MediaControls {
    let platform = PlatformConfig {
        dbus_name: "musicbee",
        display_name: "MusicBee",
        hwnd: None, // this program isn't for windows
    };

    let mut controls = MediaControls::new(platform).unwrap();

    // The closure must be Send and have a static lifetime.
    controls
        .attach(move |event| handle_event(event, &config))
        .unwrap();

    controls
}

fn handle_event(event: MediaControlEvent, config: &Config) {
    use MediaControlEvent::*;
    match event {
        Play | Pause | Toggle => run_command("/PlayPause", config),
        Next => run_command("/Next", config),
        Previous => run_command("/Previous", config),
        Stop => run_command("/Stop", config),
        _ => todo!(),
    }
}

fn run_command(command: &str, config: &Config) {
    let _ = Command::new("wine")
        .env("WINEPREFIX", &config.wine_prefix)
        .arg(&config.musicbee_location)
        .arg(command)
        .spawn();
}
