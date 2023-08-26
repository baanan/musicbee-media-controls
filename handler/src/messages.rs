use std::{time::Duration, sync::mpsc::{SyncSender, Receiver, self}, ops::ControlFlow};

use souvlaki::{MediaMetadata, MediaPlayback};
use anyhow::Result;

use crate::{listener::Listener, filesystem, config::Config};

pub enum Command {
    Exit,
    SetPlayback(MediaPlayback),
    SetMetadata(OwnedMetadata),
    SetVolume(f64),
    SetAttached(bool),
    PluginActivationUpdate(bool),
    Update,
}

impl Command {
    pub fn handle(self, listener: &mut impl Listener, tx: &MessageSender, config: &Config) -> Result<ControlFlow<()>> {
        let attached = listener.attached();
        match self {
            Command::Exit => 
                return Ok(ControlFlow::Break(())),
            Command::SetPlayback(playback) => {
                listener.playback(&playback)?;
                if config.detach_on_stop {
                    match playback {
                        MediaPlayback::Stopped => tx.detach(),
                        // FIX: the plugin doesn't know whether the listeners detached due to a
                        // stop or due to user input. This could lead to unwanted attaches.
                        _ => tx.attach(),
                    }
                }
            }, 
            Command::SetMetadata(metadata) => 
                listener.metadata(&metadata.as_ref())?, 
            Command::SetVolume(volume) => 
                listener.volume(volume)?,
            Command::SetAttached(true) if !attached => {
                listener.attach()?; 
                tx.update();
            },
            Command::SetAttached(false) if attached => 
                listener.detach()?,
            Command::PluginActivationUpdate(activated) => {
                if !activated && config.exit_with_plugin {
                    tx.exit();
                } else {
                    tx.attach_as(activated);
                }
            }
            Command::Update => 
                filesystem::update(tx, config)?,
            _ => (),
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

    pub fn playback(&self, playback: MediaPlayback) { self.send(Command::SetPlayback(playback)) }
    pub fn metadata(&self, metadata: MediaMetadata) { self.send(Command::SetMetadata(metadata.into())) }
    pub fn volume(&self, volume: f64) { self.send(Command::SetVolume(volume)) }
    pub fn attach_as(&self, attached: bool) { self.send(Command::SetAttached(attached)) }
    pub fn attach(&self) { self.attach_as(true); }
    pub fn detach(&self) { self.attach_as(false); }
    pub fn plugin_activated(&self, activated: bool) { self.send(Command::PluginActivationUpdate(activated)) }
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
            if command.handle(listener, &self.tx, config)?.is_break() {
                break;
            }
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
