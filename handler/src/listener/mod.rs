use std::sync::{Arc, Mutex};

use anyhow::Result;

use souvlaki::{MediaMetadata, MediaPlayback};

use crate::{config::Config, filesystem};

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

    pub fn add(mut self, listener: impl Listener + Send + 'static) -> Self {
        self.listeners.push(Box::new(listener));
        self
    }

    pub fn attach_if_available(mut self, config: &Config) -> Result<Self> {
        if filesystem::plugin_available(config)?.unwrap_or_default() { 
            self.attach()?; 
        }
        Ok(self)
    }

    pub fn wrap_shared(self) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(self))
    }
}

impl Listener for List {
    fn metadata(&mut self, metadata: &MediaMetadata) -> Result<()> {
        for listener in &mut self.listeners {
            listener.metadata(metadata)?;
        }
        Ok(())
    }

    fn volume(&mut self, volume: f64) -> Result<()> {
        for listener in &mut self.listeners {
            listener.volume(volume)?;
        }
        Ok(())
    }

    fn playback(&mut self, playback: &MediaPlayback) -> Result<()> {
        for listener in &mut self.listeners {
            listener.playback(playback)?;
        }
        Ok(())
    }

    fn attach(&mut self) -> Result<()> {
        for listener in &mut self.listeners {
            listener.attach()?;
        }
        Ok(())
    }

    fn detach(&mut self) -> Result<()> {
        for listener in &mut self.listeners {
            listener.detach()?;
        }
        Ok(())
    }

    fn attached(&self) -> bool {
        self.listeners.iter().all(|listener| listener.attached())
    }
}
