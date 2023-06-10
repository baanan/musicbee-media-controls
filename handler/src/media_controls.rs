use std::sync::{Arc, Mutex};

use log::*;
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, PlatformConfig, MediaPlayback, Error};

use crate::{config::Config, filesystem};

pub struct Controls {
    controls: MediaControls,
    config: Arc<Config>,
    attached: bool,
}

impl Controls {
    /// Creates new, unattached media controls
    pub fn new(config: Arc<Config>) -> Arc<Mutex<Self>> {
        let platform = PlatformConfig {
            dbus_name: "com.github.baanan.musicbee_linux",
            display_name: "MusicBee",
            hwnd: None, // windows only
        };

        let controls = MediaControls::new(platform).unwrap();

        Arc::new(Mutex::new(Self {
            controls,
            config,
            attached: false,
        }))
    }

    /// Creates new media controls and attaches
    pub fn init(config: Arc<Config>) -> Arc<Mutex<Self>> {
        let controls = Self::new(config);
        controls.lock().unwrap().attach();
        controls
    }

    /// Attaches media controls to a handler
    pub fn attach(&mut self) {
        if !self.attached {
            trace!("Attaching");

            let config = self.config.clone();

            self.controls
                .attach(move |event| handle_event(event, &config))
                .unwrap();
            self.attached = true;

            filesystem::update(self, &self.config.clone())
        } else {
            trace!("Tried to attach when already attached")
        }
    }

    /// Detatches the media controls from a handler
    pub fn detach(&mut self) {
        if self.attached {
            trace!("Detaching");
            self.controls.detach().unwrap();
            self.attached = false;
        } else {
            trace!("Tried to detach when not attached")
        }
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
    trace!("Recieved control event: {event:?}");

    use MediaControlEvent::*;
    match event {
        Play | Pause | Toggle => config.run_command("/PlayPause"),
        Next => config.run_command("/Next"),
        Previous => config.run_command("/Previous"),
        Stop => config.run_command("/Stop"),
        _ => { error!("Event {event:?} not implemented") } // TODO: implement other events
    }
}
