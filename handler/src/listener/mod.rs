
use anyhow::Result;

use async_trait::async_trait;
use futures::{future::join_all, Future};
use log::{trace, debug};
use souvlaki::{MediaMetadata, MediaPlayback};
use futures::FutureExt;

pub mod media_controls;
pub mod rpc;

#[async_trait]
pub trait Listener {
    /// Called when the metadata is updated
    ///
    /// The `metadata`'s cover is guaranteed to be a valid [`Url`](url::Url)
    async fn metadata(&mut self, metadata: &MediaMetadata) -> Result<()>;
    /// Called when the volume is updated
    async fn volume(&mut self, volume: f64) -> Result<()>;
    /// Called when the playback is updated
    async fn playback(&mut self, playback: &MediaPlayback) -> Result<()>;

    /// Attach / resume the listener
    async fn attach(&mut self) -> Result<()>;
    /// Detach / pause the listener
    async fn detach(&mut self) -> Result<()>;

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

    async fn update_all<'a, Fut, F>(&'a mut self, func: F) -> Result<()> 
    where 
        F: Fn(&'a mut Box<dyn Listener + Send>) -> Fut,
        Fut: Future<Output = Result<()>> + 'a
    {
        let updates = self.listeners.iter_mut()
            .map(func);
        join_all(updates).await.into_iter()
            .collect::<Result<()>>()?;
        Ok(())
    }
}

#[async_trait]
impl Listener for List {
    async fn metadata(&mut self, metadata: &MediaMetadata) -> Result<()> {
        debug!("updating metadata: {} - {}", metadata.artist.unwrap_or_default(), metadata.title.unwrap_or_default());
        self.update_all(|listener| listener.metadata(metadata)).await
    }

    async fn volume(&mut self, volume: f64) -> Result<()> {
        debug!("updating volume: {volume}");
        self.update_all(|listener| listener.volume(volume)).await
    }

    async fn playback(&mut self, playback: &MediaPlayback) -> Result<()> {
        debug!("updating playback: {}", display_playback(playback));
        self.update_all(|listener| listener.playback(playback)).await
    }

    async fn attach(&mut self) -> Result<()> {
        trace!("Attaching");
        self.update_all(|listener| { 
            if !listener.attached() { 
                return listener.attach();
            }
            futures::future::ready(Ok(())).boxed()
        }).await
    }

    async fn detach(&mut self) -> Result<()> {
        trace!("Detaching");
        self.update_all(|listener| { 
            if listener.attached() { 
                return listener.detach();
            }
            futures::future::ready(Ok(())).boxed()
        }).await
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
