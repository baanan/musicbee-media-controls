use std::{time::Duration, sync::mpsc::{SyncSender, Receiver, self}, ops::ControlFlow};

use log::error;
use souvlaki::{MediaMetadata, MediaPlayback};
use anyhow::{Result, Context};

use crate::{listener::Listener, filesystem, config::Config};

pub enum Command {
    Exit,
    Playback(MediaPlayback),
    Metadata(OwnedMetadata),
    Volume(f64),
    Attached(bool),
    PluginActivated(bool),
    Update,
    UpdatePlayback,
    UpdateMetadata,
    UpdateVolume,
    UpdatePluginActivation,
}

impl Command {
    pub fn handle(self, listener: &mut impl Listener, tx: &MessageSender, config: &Config) -> Result<ControlFlow<()>> {
        let attached = listener.attached();
        match self {
            Command::Exit => 
                return Ok(ControlFlow::Break(())),

            Command::Metadata(metadata) => 
                listener.metadata(&metadata.as_ref()).context("failed to set metadata")?, 
            Command::Playback(playback) => {
                listener.playback(&playback).context("failed to set playback")?;

                if config.detach_on_stop {
                    match playback {
                        MediaPlayback::Stopped => tx.detach(),
                        // FIX: the plugin doesn't know whether the listeners detached due to a
                        // stop or due to user input. This could lead to unwanted attaches.
                        _ => tx.attach(),
                    }
                }
            }, 
            Command::Volume(volume) => 
                listener.volume(volume).context("failed to set volume")?,
            Command::Attached(true) if !attached => {
                listener.attach().context("failed to attach")?; 
                tx.update();
            },
            Command::Attached(false) if attached => 
                listener.detach().context("failed to detach")?,
            // ignore attaches when already attached and detaches when already detached
            Command::Attached(_) => (),
            Command::PluginActivated(activated) => {
                if !activated && config.exit_with_plugin {
                    tx.exit();
                } else {
                    tx.attach_as(activated);
                }
            }

            Command::Update => 
                filesystem::update(tx, config).context("failed to update handlers")?,
            Command::UpdateMetadata => 
                filesystem::update_metadata(tx, config).context("failed to update metadata")?,
            Command::UpdatePlayback => 
                filesystem::update_playback(tx, config).context("failed to update playback")?,
            Command::UpdateVolume => 
                filesystem::update_volume(tx, config).context("failed to update volume")?,
            Command::UpdatePluginActivation => 
                filesystem::plugin_activation_changed(tx, config).context("failed to update plugin activation")?,
        }
        Ok(ControlFlow::Continue(()))
    }
}

#[derive(Clone)]
pub struct MessageSender { tx: SyncSender<Command> }

impl MessageSender {
    fn send(&self, command: Command) {
        self.tx.send(command)
            .expect("message reciever hung up before program ended");
    }

    pub fn exit(&self) { self.send(Command::Exit) }
    pub fn update(&self) { self.send(Command::Update) }

    pub fn playback(&self, playback: MediaPlayback) { self.send(Command::Playback(playback)) }
    pub fn metadata(&self, metadata: MediaMetadata) { self.send(Command::Metadata(metadata.into())) }
    pub fn volume(&self, volume: f64) { self.send(Command::Volume(volume)) }
    pub fn plugin_activated(&self, activated: bool) { self.send(Command::PluginActivated(activated)) }

    pub fn attach_as(&self, attached: bool) { self.send(Command::Attached(attached)) }
    pub fn attach(&self) { self.attach_as(true); }
    pub fn detach(&self) { self.attach_as(false); }

    pub fn update_metadata(&self) { self.send(Command::UpdateMetadata); }
    pub fn update_playback(&self) { self.send(Command::UpdatePlayback); }
    pub fn update_volume(&self) { self.send(Command::UpdateVolume); }
    pub fn update_plugin_activation(&self) { self.send(Command::UpdatePluginActivation); }
}

pub struct Messages { tx: MessageSender, rx: Receiver<Command> }

impl Messages {
    pub fn new() -> Messages {
        // it's important that the channel has some
        // space in its buffer so that the filesystem
        // doesn't block in filesystem::plugin_activation_changed.
        // that could create a deadlock when the daemon exits,
        // and tries to lock the listeners to detach them
        let (tx, rx) = mpsc::sync_channel(8);
        Self { tx: MessageSender { tx }, rx }
    }

    /// Returns a [clone](Clone) of the [`MessageSender`]
    pub fn sender(&self) -> MessageSender { self.tx.clone() }

    /// Listens to commands until [`Command::Exit`] is sent
    pub fn listen_until_exit(self, listener: &mut impl Listener, config: &Config) -> Result<()> {
        for command in self.rx {
            let result = command.handle(listener, &self.tx, config)
                .unwrap_or_else(|err| {
                    error!("failed to handle command: {err:?}");
                    ControlFlow::Continue(())
                });
            if result.is_break() { break; }
        }
        Ok(())
    }
}

pub struct OwnedMetadata {
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub cover_url: Option<String>,
    pub duration: Option<Duration>,
}

impl OwnedMetadata {
    pub fn as_ref(&self) -> MediaMetadata<'_> {
        let OwnedMetadata { title, album, artist, cover_url, duration } = self;
        MediaMetadata {
            title: title.as_deref(),
            album: album.as_deref(),
            artist: artist.as_deref(),
            cover_url: cover_url.as_deref(),
            duration: *duration,
        }
    }
}

// this is ugly, because souvlaki turns a MediaMetadata into its own form of OwnedMetadata
// therefore, ToOwned unnecessarily gets called twice
// but I can't think of another way to do it
impl<'a> From<MediaMetadata<'a>> for OwnedMetadata {
    fn from(MediaMetadata { title, album, artist, cover_url, duration }: MediaMetadata) -> Self {
        Self {
            title: title.map(ToOwned::to_owned),
            album: album.map(ToOwned::to_owned),
            artist: artist.map(ToOwned::to_owned),
            cover_url: cover_url.map(ToOwned::to_owned),
            duration
        }
    }
}
