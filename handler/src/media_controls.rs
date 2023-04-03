use std::{process::Command, sync::{Arc, Mutex}};

use log::debug;
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, PlatformConfig, MediaPlayback, Error};

use crate::{config::Config, filesystem};

pub struct Controls {
    controls: MediaControls,
    config: Config,
}

impl Controls {
    /// Creates new, unattached media controls
    pub fn new(config: Config) -> Arc<Mutex<Self>> {
        let platform = PlatformConfig {
            dbus_name: "musicbee",
            display_name: "MusicBee",
            hwnd: None, // windows only
        };

        let controls = MediaControls::new(platform).unwrap();

        Arc::new(Mutex::new(Self {
            controls,
            config
        }))
    }

    /// Creates new media controls and attaches
    pub fn init(config: Config) -> Arc<Mutex<Self>> {
        let controls = Self::new(config);
        controls.lock().unwrap().attach();
        controls
    }

    /// Attaches media controls to a handler
    pub fn attach(&mut self) {
        let config = self.config.clone();

        self.controls
            .attach(move |event| handle_event(event, &config))
            .unwrap();

        // TODO: move this function to an Arc<Mutex<Self>> impl
        // filesystem::update(self, &self.config);
    }

    /// Detatches the media controls from a handler
    pub fn detach(&mut self) {
        self.controls.detach().unwrap();
    }

    /// Delegate to set the metadata of the controls
    pub fn set_metadata(&mut self, metadata: MediaMetadata) -> Result<(), Error> {
        self.controls.set_metadata(metadata)
    }

    // / Delegate to set the playback of the controls
    pub fn set_playback(&mut self, playback: MediaPlayback) -> Result<(), Error> {
        self.controls.set_playback(playback)
    }
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
    let mut cmd = Command::new("wine");
     cmd.env("WINEPREFIX", &config.wine_prefix)
        .arg(&config.musicbee_location)
        .arg(command);

    debug!("Running command: \n    WINEPREFIX={} wine {} {}", config.wine_prefix, config.musicbee_location, command);

    let _ = cmd
        .spawn().unwrap()
        .wait().unwrap();
}
