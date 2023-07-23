#![allow(clippy::similar_names)]
use std::{sync::{Arc, Mutex}, time::Duration, path::{PathBuf, Path}};

use log::{debug, error, trace, warn};
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, PlatformConfig, MediaPlayback, Error, SeekDirection, MediaPosition};
use url::Url;

use crate::{config::Config, filesystem, communication::Action};

pub struct Controls {
    controls: MediaControls,
    config: Arc<Config>,
    pub attached: bool,
}

impl Controls {
    /// Creates new, unattached media controls
    pub fn new(config: Arc<Config>) -> Arc<Mutex<Self>> {
        let platform = PlatformConfig {
            dbus_name: "com.github.baanan.musicbee_linux",
            display_name: "MusicBee",
            hwnd: None, // windows only
        };

        let controls = MediaControls::new(platform)
            .expect("to be able to create the media controls");

        Arc::new(Mutex::new(Self {
            controls,
            config,
            attached: false,
        }))
    }

    /// Creates new media controls and attaches if the plugin is available
    pub fn init(config: Arc<Config>) -> Arc<Mutex<Self>> {
        let plugin_available = filesystem::plugin_available(&config);

        let controls = Self::new(config);
        if plugin_available { controls.lock().unwrap().attach(); }
        controls
    }

    /// Attaches media controls to a handler
    pub fn attach(&mut self) {
        if self.attached {
            trace!("Tried to attach when already attached"); return;
        }

        trace!("Attaching");

        let config = self.config.clone();

        self.controls
            .attach(move |event| handle_event(event, &config))
            .expect("to be able to attach the media controls");
        self.attached = true;

        filesystem::update(self, &self.config.clone());
    }

    /// Detatches the media controls from a handler
    pub fn detach(&mut self) {
        if !self.attached {
            trace!("Tried to detach when not attached"); return;
        }

        trace!("Detaching");
        self.controls.detach()
            .expect("to be able to detach the media controls");
        self.attached = false;
    }

    /// Delegate to set the metadata of the controls
    pub fn set_metadata(&mut self, metadata: MediaMetadata) -> Result<(), Error> {
        if self.attached { self.controls.set_metadata(metadata)?; }
        Ok(())
    }

    /// Delegate to set the playback of the controls
    pub fn set_playback(&mut self, playback: MediaPlayback) -> Result<(), Error> {
        if self.config.detach_on_stop { 
            match playback {
                MediaPlayback::Stopped if self.attached => self.detach(),
                MediaPlayback::Playing { .. } if !self.attached => self.attach(),
                _ => {},
            }
        }

        if self.attached { self.controls.set_playback(playback)?; }
        Ok(())
    }
}

fn handle_event(event: MediaControlEvent, config: &Config) {
    #[allow(clippy::enum_glob_use)]
    use MediaControlEvent::*;
    debug!("Recieved control event: {event:?}");
    match event {
        Play | Pause | Toggle => config.run_command("/PlayPause", None),
        Next => config.run_command("/Next", None),
        Previous => config.run_command("/Previous", None),
        Stop => config.run_command("/Stop", None),
        OpenUri(uri) => config.run_command("/Play", Some(map_uri(uri))),
        Seek(direction) => directioned_duration_to_seek(direction, config.seek_amount).run(config),
        SeekBy(direction, duration) => directioned_duration_to_seek(direction, duration).run(config),
        SetPosition(MediaPosition(pos)) => Action::Position(pos).run(config),
        _ => { error!("Event {event:?} not implemented") } // TODO: implement other events
    }
}

fn directioned_duration_to_seek(direction: SeekDirection, duration: Duration) -> Action {
    let duration: i32 = duration.as_millis().try_into()
        .expect("the duration to fit inside an i32");

    let milis = match direction {
        SeekDirection::Forward => duration,
        SeekDirection::Backward => -duration,
    };

    Action::Seek { milis }
}

fn map_uri(uri: String) -> String {
    let url = Url::parse(&uri);
    match url {
        Ok(url) if url.scheme() == "file" => map_file_uri(&url),
        Ok(url) => url.to_string(),
        Err(_) => {
            trace!("uri given was not a valid uri, defaulting to file");
            uri
        }
    }
}

fn map_file_uri(uri: &Url) -> String {
    if let Ok(uri) = uri.to_file_path() {
        if let Some(uri) = uri.to_str() {
            return format!("Z:{uri}");
        }
    }

    warn!("could not get path from file url");
    uri.to_string()
}
