
use anyhow::Result;

use log::{trace, debug};
use souvlaki::{MediaMetadata, MediaPlayback};

pub mod media_controls;
pub mod rpc;

pub trait Listener {
    /// Called when the metadata is updated
    ///
    /// The `metadata`'s cover is guaranteed to be a valid [`Url`](url::Url)
    fn metadata(&mut self, metadata: &MediaMetadata) -> Result<()>;
    /// Called when the volume is updated
    fn volume(&mut self, volume: f64) -> Result<()>;
    /// Called when the playback is updated
    fn playback(&mut self, playback: &MediaPlayback) -> Result<()>;

    /// Attach / resume the listener
    fn attach(&mut self) -> Result<()>;
    /// Detach / pause the listener
    fn detach(&mut self) -> Result<()>;

    fn attached(&self) -> bool;
}

#[derive(Default)]
pub struct List {
    listeners: Vec<Box<dyn Listener + Send>>,
}

impl List {
    pub fn new() -> Self { Self::default() }

    pub fn add(&mut self, listener: impl Listener + Send + 'static) {
        self.listeners.push(Box::new(listener));
    }
}

impl Listener for List {
    fn metadata(&mut self, metadata: &MediaMetadata) -> Result<()> {
        debug!("updating metadata: {} - {}", metadata.artist.unwrap_or_default(), metadata.title.unwrap_or_default());
        for listener in &mut self.listeners {
            listener.metadata(metadata)?;
        }
        Ok(())
    }

    fn volume(&mut self, volume: f64) -> Result<()> {
        debug!("updating volume: {volume}");
        for listener in &mut self.listeners {
            listener.volume(volume)?;
        }
        Ok(())
    }

    fn playback(&mut self, playback: &MediaPlayback) -> Result<()> {
        debug!("updating playback: {}", display_playback(playback));
        for listener in &mut self.listeners {
            listener.playback(playback)?;
        }
        Ok(())
    }

    fn attach(&mut self) -> Result<()> {
        trace!("Attaching");
        for listener in &mut self.listeners {
            if !listener.attached() { listener.attach()?; }
        }
        Ok(())
    }

    fn detach(&mut self) -> Result<()> {
        trace!("Detaching");
        for listener in &mut self.listeners {
            if listener.attached() { listener.detach()?; }
        }
        Ok(())
    }

    fn attached(&self) -> bool {
        self.listeners.iter().all(|listener| listener.attached())
    }
}

fn display_playback(playback: &MediaPlayback) -> &'static str {
    match playback {
        MediaPlayback::Stopped => "stopped",
        MediaPlayback::Paused { .. } => "paused",
        MediaPlayback::Playing { .. } => "playing",
    }
}
