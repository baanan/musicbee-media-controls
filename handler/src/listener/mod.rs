
use anyhow::Result;

use async_trait::async_trait;
use futures::future::join_all;
use log::{error, debug};
use souvlaki::MediaPlayback;
use tokio::sync::broadcast::Receiver;

use crate::{messages::Command, config::Config};

pub mod media_controls;
pub mod rpc;

#[async_trait]
pub trait Listener {
    async fn handle(&mut self, command: Command, config: &Config) -> Result<()>;

    async fn listen(mut self: Box<Self>, mut reciever: Receiver<Command>, config: &Config) {
        while let Ok(command) = reciever.recv().await {
            if matches!(command, Command::Exit) {
                debug!("{} exited", self.name());
                break;
            }

            self.handle(command, config).await
                .unwrap_or_else(|err| error!("{} failed to handle command: {err}", self.name()));
        }
    }

    fn name(&self) -> &'static str;
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

    pub async fn listen(self, reciever: Receiver<Command>, config: &Config) {
        let futures = self.listeners.into_iter()
            .map(|listener| listener.listen(reciever.resubscribe(), config));
        join_all(futures).await;
    }
}

pub struct Logger;
#[async_trait]
impl Listener for Logger {
    async fn handle(&mut self, command: Command, _: &Config) -> Result<()> {
        match command {
            Command::Exit => debug!("exit command recieved"),
            Command::Metadata(metadata) => debug!(
                "updating metadata: {} - {}",
                metadata.artist.as_deref().unwrap_or_default(),
                metadata.title.as_deref().unwrap_or_default()
            ),
            Command::Playback(playback) => debug!("updating playback: {}", display_playback(&playback)),
            Command::Attached(true) => debug!("attaching..."),
            Command::Attached(false) => debug!("detaching..."),
            Command::Volume(vol) => debug!("updating volume: {vol}"),
            _ => (),
        }
        Ok(())
    }

    fn name(&self) -> &'static str { "logger" }
}

const fn display_playback(playback: &MediaPlayback) -> &'static str {
    match playback {
        MediaPlayback::Stopped => "stopped",
        MediaPlayback::Paused { .. } => "paused",
        MediaPlayback::Playing { .. } => "playing",
    }
}
