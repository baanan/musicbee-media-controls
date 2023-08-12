use std::sync::{Arc, Mutex};

use anyhow::{Result, Context};

use log::trace;
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
    fn playback_inner(&mut self, playback: &MediaPlayback) -> Result<()>;

    /// Attach / resume the listener
    fn attach(&mut self) -> Result<()>;
    /// Detach / pause the listener
    fn detach(&mut self) -> Result<()>;

    fn attach_and_update(&mut self, config: &Config) -> Result<()> where Self: Sized {
        self.attach()?;
        filesystem::update(self, config)
            .context("failed to update listeners after attach")?;
        Ok(())
    }

    fn playback(&mut self, playback: &MediaPlayback, config: &Config) -> Result<()> where Self: Sized {
        if config.detach_on_stop { 
            let attached = self.attached();
            match playback {
                MediaPlayback::Stopped if attached => self.detach()?,
                MediaPlayback::Playing { .. } if !attached => self.attach_and_update(config)?,
                _ => {},
            }
        }
        self.playback_inner(playback)
    }

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

    pub fn attach_if_available(mut self, config: &Config) -> Result<Self> {
        if filesystem::plugin_available(config)?.unwrap_or_default() { 
            self.attach_and_update(config)?; 
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

    fn playback_inner(&mut self, playback: &MediaPlayback) -> Result<()> {
        for listener in &mut self.listeners {
            listener.playback_inner(playback)?;
        }
        Ok(())
    }

    fn attach(&mut self) -> Result<()> {
        for listener in &mut self.listeners {
            if !listener.attached() { listener.attach()?; }
        }
        Ok(())
    }

    fn detach(&mut self) -> Result<()> {
        trace!("recieved detach");
        for listener in &mut self.listeners {
            if listener.attached() { listener.detach()?; }
        }
        Ok(())
    }

    fn attached(&self) -> bool {
        self.listeners.iter().all(|listener| listener.attached())
    }
}
