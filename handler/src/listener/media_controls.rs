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

#[async_trait]
impl Listener for Controls {
    async fn handle(&mut self, command: Command, config: &Config) -> Result<()> {
        match command {
            Command::Metadata(metadata) => 
                self.metadata(&(*metadata).as_ref()).context("failed to set metadata")?, 
            Command::Playback(playback) => 
                self.playback(&playback).context("failed to set playback")?, 
            Command::Volume(volume) => 
                self.volume(volume).context("failed to set volume")?,
            Command::Attached(true) if !self.attached =>
                self.attach().context("failed to attach")?,
            Command::Attached(false) if self.attached => 
                self.detach().context("failed to detach")?,

            Command::MediaControlEvent(event) =>
                handle_event(&event, config).await.context("failed to handle event")?,
            // NOTE: ignores attaches when already attached and detaches when already detached
            _ => (),
        }
        Ok(())
    }

    fn name(&self) -> &'static str { "media controls" }
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

    /// Attaches media controls to a handler
    fn attach(&mut self) -> Result<()> {
        assert!(!self.attached, "can only attach when not already attached");

        let sender = self.sender.clone();
        self.controls
            .attach(move |event| sender.media_control_event(event))
            .map_err(ControlsError::from)?;
        self.attached = true;

        Ok(())
    }

    /// Detatches the media controls from a handler
    fn detach(&mut self) -> Result<()> {
        assert!(self.attached, "can only detach when attached");

        self.controls.detach().map_err(ControlsError::from)?;
        self.attached = false;

        Ok(())
    }

    /// Delegate to set the metadata of the controls
    fn metadata(&mut self, metadata: &MediaMetadata<'_>) -> Result<()> {
        if self.attached { 
            self.controls.set_metadata(metadata.clone()).map_err(ControlsError::from)?; 
        }
        Ok(())
    }

    /// Delegate to set the volume of the controls
    fn volume(&mut self, volume: f64) -> Result<()> {
        if self.attached { 
            self.controls.set_volume(volume).map_err(ControlsError::from)?;
        }
        Ok(())
    }

    /// Delegate to set the playback of the controls
    fn playback(&mut self, playback: &MediaPlayback) -> Result<()> {
        if self.attached { 
            self.controls.set_playback(playback.clone()).map_err(ControlsError::from)?; 
        }
        Ok(())
    }
}

pub async fn handle_event(event: &MediaControlEvent, config: &Config) -> Result<()> {
    #[allow(clippy::enum_glob_use)]
    use MediaControlEvent::*;
    debug!("Recieved control event: {event:?}");
    match event {
        Play | Pause | Toggle => config.run_simple_command("/PlayPause")?,
        Next => config.run_simple_command("/Next")?,
        Previous => config.run_simple_command("/Previous")?,
        Stop => config.run_simple_command("/Stop")?,
        OpenUri(uri) => config.run_command("/Play", Some(map_uri(uri)))?,
        Seek(direction) => directioned_duration_to_seek(*direction, config.media_controls.seek_amount)?
            .run(config).await?,
        SeekBy(direction, duration) => directioned_duration_to_seek(*direction, *duration)?
            .run(config).await?,
        SetPosition(MediaPosition(pos)) => Action::Position(*pos).run(config).await?,
        SetVolume(vol) => if config.media_controls.send_volume { Action::Volume(*vol).run(config).await? },
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

fn map_uri(uri: &str) -> String {
    let url = Url::parse(uri);
    match url {
        Ok(url) if url.scheme() == "file" => map_file_uri(&url),
        Ok(url) => url.to_string(),
        Err(_) => {
            trace!("uri given was not a valid uri, defaulting to file");
            uri.to_string()
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
