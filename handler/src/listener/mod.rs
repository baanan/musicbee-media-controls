use std::sync::{Arc, Mutex};

use anyhow::Result;

use souvlaki::{MediaMetadata, MediaPlayback};

pub mod media_controls;

pub trait Listener {
    fn set_metadata(&mut self, metadata: &MediaMetadata) -> Result<()>;
    fn set_volume(&mut self, volume: f64) -> Result<()>;
    fn set_playback(&mut self, playback: &MediaPlayback) -> Result<()>;

    fn attach(&mut self) -> Result<()>;
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

    pub fn wrap_shared(self) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(self))
    }
}

impl Listener for List {
    fn set_metadata(&mut self, metadata: &MediaMetadata) -> Result<()> {
        for listener in &mut self.listeners {
            listener.set_metadata(metadata)?;
        }
        Ok(())
    }

    fn set_volume(&mut self, volume: f64) -> Result<()> {
        for listener in &mut self.listeners {
            listener.set_volume(volume)?;
        }
        Ok(())
    }

    fn set_playback(&mut self, playback: &MediaPlayback) -> Result<()> {
        for listener in &mut self.listeners {
            listener.set_playback(playback)?;
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
