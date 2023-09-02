#![allow(clippy::similar_names)]
use std::time::Duration;

use anyhow::{Result, Context};
use async_trait::async_trait;
use log::*;
use souvlaki::*;
use thiserror::Error;
use url::Url;

use crate::{config::Config, communication::Action, messages::{MessageSender, Command}};

use super::Listener;

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
    sender: MessageSender,
    attached: bool,
}

impl Controls {
    /// Creates new, unattached media controls
    pub fn new(sender: MessageSender) -> ControlsResult<Self> {
        let platform = PlatformConfig {
            dbus_name: "com.github.baanan.musicbee_linux",
            display_name: "MusicBee",
            hwnd: None, // windows only
        };

        let controls = MediaControls::new(platform)?;

        Ok(Self {
            controls,
            sender,
            attached: false,
        })
    }
}

#[async_trait]
impl Listener for Controls {
    /// Attaches media controls to a handler
    async fn attach(&mut self) -> Result<()> {
        if self.attached {
            return Err(ControlsError::AlreadyAttached)?;
        }

        let sender = self.sender.clone();
        self.controls
            .attach(move |event| sender.blocking_send(Command::MediaControlEvent(event)))
            .map_err(ControlsError::from)?;
        self.attached = true;

        Ok(())
    }

    /// Detatches the media controls from a handler
    async fn detach(&mut self) -> Result<()> {
        if !self.attached {
            return Err(ControlsError::AlreadyDetached)?;
        }

        self.controls.detach().map_err(ControlsError::from)?;
        self.attached = false;

        Ok(())
    }

    /// Delegate to set the metadata of the controls
    async fn metadata(&mut self, metadata: &MediaMetadata) -> Result<()> {
        if self.attached { 
            self.controls.set_metadata(metadata.clone()).map_err(ControlsError::from)?; 
        }
        Ok(())
    }

    /// Delegate to set the volume of the controls
    async fn volume(&mut self, volume: f64) -> Result<()> {
        if self.attached { 
            self.controls.set_volume(volume).map_err(ControlsError::from)?;
        }
        Ok(())
    }

    /// Delegate to set the playback of the controls
    async fn playback(&mut self, playback: &MediaPlayback) -> Result<()> {
        if self.attached { 
            self.controls.set_playback(playback.clone()).map_err(ControlsError::from)?; 
        }
        Ok(())
    }

    fn attached(&self) -> bool { self.attached }
}

pub async fn handle_event(event: MediaControlEvent, config: &Config) -> Result<()> {
    #[allow(clippy::enum_glob_use)]
    use MediaControlEvent::*;
    debug!("Recieved control event: {event:?}");
    match event {
        Play | Pause | Toggle => config.run_simple_command("/PlayPause")?,
        Next => config.run_simple_command("/Next")?,
        Previous => config.run_simple_command("/Previous")?,
        Stop => config.run_simple_command("/Stop")?,
        OpenUri(uri) => config.run_command("/Play", Some(map_uri(uri)))?,
        Seek(direction) => directioned_duration_to_seek(direction, config.media_controls.seek_amount)?
            .run(config).await?,
        SeekBy(direction, duration) => directioned_duration_to_seek(direction, duration)?
            .run(config).await?,
        SetPosition(MediaPosition(pos)) => Action::Position(pos).run(config).await?,
        SetVolume(vol) => if config.media_controls.send_volume { Action::Volume(vol).run(config).await? },
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
