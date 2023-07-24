#![allow(clippy::similar_names)]
use std::{sync::{Arc, Mutex}, time::Duration};

use anyhow::{Result, Context};
use log::*;
use souvlaki::*;
use thiserror::Error;
use url::Url;

use crate::{config::Config, filesystem, communication::Action};

#[derive(Debug, Error)]
pub enum ControlsError {
    #[error("tried to attach when already attached")]
    AlreadyAttached,
    #[error("tried to detach when already detached")]
    AlreadyDetached,
    #[error("system error: {0:?}")]
    System(souvlaki::Error)
}

impl From<souvlaki::Error> for ControlsError {
    fn from(v: souvlaki::Error) -> Self {
        Self::System(v)
    }
}

pub type ControlsResult<T> = Result<T, ControlsError>;

pub struct Controls {
    controls: MediaControls,
    config: Arc<Config>,
    volume: f64,
    pub attached: bool,
}

impl Controls {
    /// Creates new, unattached media controls
    pub fn new(config: Arc<Config>) -> ControlsResult<Arc<Mutex<Self>>> {
        let platform = PlatformConfig {
            dbus_name: "com.github.baanan.musicbee_linux",
            display_name: "MusicBee",
            hwnd: None, // windows only
        };

        let controls = MediaControls::new(platform)?;

        Ok(Arc::new(Mutex::new(Self {
            controls,
            config,
            attached: false,
            volume: 0.0,
        })))
    }

    /// Creates new media controls and attaches if the plugin is available
    pub fn init(config: Arc<Config>) -> Result<Arc<Mutex<Self>>> {
        let plugin_available = filesystem::plugin_available(&config)?;
        let plugin_available = plugin_available.is_some_and(|f| f);

        let controls = Self::new(config)?;
        if plugin_available { controls.lock().unwrap().attach()?; }
        Ok(controls)
    }

    /// Attaches media controls to a handler
    pub fn attach(&mut self) -> Result<()> {
        if self.attached {
            return Err(ControlsError::AlreadyAttached)?;
        }

        trace!("Attaching");

        let config = self.config.clone();
        self.controls.attach(move |event| 
            handle_event(event, &config)
                .unwrap_or_else(|err| error!("failed to handle event: {err}"))
        ).map_err(ControlsError::from)?;
        self.attached = true;

        filesystem::update(self, &self.config.clone())?;
        Ok(())
    }

    /// Detatches the media controls from a handler
    pub fn detach(&mut self) -> ControlsResult<()> {
        if !self.attached {
            return Err(ControlsError::AlreadyDetached);
        }

        trace!("Detaching");
        self.controls.detach()?;
        self.attached = false;

        Ok(())
    }

    /// Delegate to set the metadata of the controls
    pub fn set_metadata(&mut self, metadata: MediaMetadata) -> ControlsResult<()> {
        if self.attached { self.controls.set_metadata(metadata)?; }
        Ok(())
    }

    /// Delegate to set the volume of the controls
    pub fn set_volume(&mut self, volume: f64) -> ControlsResult<()> {
        self.controls.set_volume(volume)?;
        Ok(())
    }

    /// Determines whether the input `volume` is different than the current volume
    pub fn volume_is_new(&self, volume: f64) -> bool {
        (volume - self.volume).abs() > 0.01
    }

    /// Delegate to set the playback of the controls
    pub fn set_playback(&mut self, playback: MediaPlayback) -> Result<()> {
        if self.config.detach_on_stop { 
            match playback {
                MediaPlayback::Stopped if self.attached => self.detach()?,
                MediaPlayback::Playing { .. } if !self.attached => self.attach()?,
                _ => {},
            }
        }

        if self.attached { self.controls.set_playback(playback).map_err(ControlsError::from)?; }
        Ok(())
    }
}

fn handle_event(event: MediaControlEvent, config: &Config) -> Result<()> {
    #[allow(clippy::enum_glob_use)]
    use MediaControlEvent::*;
    debug!("Recieved control event: {event:?}");
    match event {
        Play | Pause | Toggle => config.run_simple_command("/PlayPause")?,
        Next => config.run_simple_command("/Next")?,
        Previous => config.run_simple_command("/Previous")?,
        Stop => config.run_simple_command("/Stop")?,
        OpenUri(uri) => config.run_command("/Play", Some(map_uri(uri)))?,
        Seek(direction) => directioned_duration_to_seek(direction, config.seek_amount)?
            .run(config)?,
        SeekBy(direction, duration) => directioned_duration_to_seek(direction, duration)?
            .run(config)?,
        SetPosition(MediaPosition(pos)) => Action::Position(pos).run(config)?,
        SetVolume(vol) => Action::Volume(vol).run(config)?,
        _ => { error!("Event {event:?} not implemented") } // TODO: implement other events
    }
    Ok(())
}

fn directioned_duration_to_seek(direction: SeekDirection, duration: Duration) -> Result<Action> {
    let duration: i32 = duration.as_millis().try_into()
        .context("failed to convert the seek duration into an i32")?;

    let milis = match direction {
        SeekDirection::Forward => duration,
        SeekDirection::Backward => -duration,
    };

    Ok(Action::Seek { milis })
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
