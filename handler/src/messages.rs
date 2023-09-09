use std::sync::Arc;
use std::time::Duration;

use log::error;
use souvlaki::{MediaMetadata, MediaPlayback, MediaControlEvent};
use anyhow::{Result, Context};
use tokio::sync::mpsc::{self, Sender, Receiver};

use crate::{listener::{Listener, media_controls}, filesystem, config::Config};

/// Adds blocking versions (in the form of blocking_name) of a list of async methods
macro_rules! add_blocking {
    (
        $(
            pub async fn $name:ident(&$self:tt $(, $arg:ident: $ty:ty)* $(,)?) $(-> $ret:ty)? { $($body:tt)* }
        )*
    ) => {
        $(
            #[allow(dead_code)] pub async fn $name(&$self $(, $arg: $ty)*) $(-> $ret)? { $($body)* }
            paste::paste!{ 
                #[allow(dead_code)] pub fn [<blocking_ $name>](&$self $(, $arg: $ty)*) $(-> $ret)? {
                    futures::executor::block_on($self.$name($($arg),*))
                }
            }
        )*
    };
}

#[derive(Debug)]
pub enum Command {
    Exit,
    Playback(Arc<MediaPlayback>),
    Metadata(Arc<OwnedMetadata>),
    Volume(f64),
    Attached(bool),
    Update,
    UpdatePlayback,
    UpdateMetadata,
    UpdateVolume,
    UpdatePluginActivation,
    MediaControlEvent(Arc<MediaControlEvent>),
}

impl Command {
    pub async fn handle(self, listener: &mut impl Listener, tx: &MessageSender, config: Arc<Config>) -> Result<()> {
        let attached = listener.attached();
        match self {
            Command::Exit => (),

            Command::Metadata(metadata) => 
                listener.metadata(&(*metadata).as_ref()).await.context("failed to set metadata")?, 
            Command::Playback(playback) => 
                listener.playback(&playback).await.context("failed to set playback")?, 
            Command::Volume(volume) => 
                listener.volume(volume).await.context("failed to set volume")?,
            Command::Attached(true) if !attached =>
                listener.attach().await.context("failed to attach")?,
            Command::Attached(false) if attached => 
                listener.detach().await.context("failed to detach")?,
            // ignore attaches when already attached and detaches when already detached
            Command::Attached(_) => (),

            Command::Update => 
                filesystem::update(tx, &config).await.context("failed to update handlers")?,
            Command::UpdateMetadata => 
                filesystem::update_metadata(tx, &config).await.context("failed to update metadata")?,
            Command::UpdatePlayback => 
                filesystem::update_playback(tx, &config).await.context("failed to update playback")?,
            Command::UpdateVolume => 
                filesystem::update_volume(tx, &config).await.context("failed to update volume")?,
            Command::UpdatePluginActivation => 
                filesystem::plugin_activation_changed(tx, &config).await.context("failed to update plugin activation")?,

            Command::MediaControlEvent(event) =>
                media_controls::handle_event(&event, &config).await.context("failed to handle event")?,
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct MessageSender {
    tx: Sender<Command>,
    config: Arc<Config>,
}

impl MessageSender {
    async fn send(&self, command: Command) {
        self.tx.send(command).await
            .expect("message reciever hung up before program ended");
    }

    // pub fn blocking_send(&self, command: Command) {
    //     self.tx.blocking_send(command)
    //         .expect("message reciever hung up before program ended");
    // }

    add_blocking! {
        pub async fn exit(&self) {
            self.detach().await;
            self.send(Command::Exit).await
        }
        pub async fn update(&self) { self.send(Command::Update).await }

        pub async fn playback(&self, playback: MediaPlayback) {
            if self.config.detach_on_stop {
                match playback {
                    MediaPlayback::Stopped => self.detach().await,
                    // the attach can't also update, or it could create an infinite loop of
                    // checking the playback and updating
                    // FIX: the plugin doesn't know whether the listeners detached due to a
                    // stop or due to user input. This could lead to unwanted attaches.
                    _ => self.attach_without_update().await,
                }
            }
            self.send(Command::Playback(Arc::new(playback))).await
        }
        pub async fn metadata(&self, metadata: MediaMetadata<'_>) { self.send(Command::Metadata(Arc::new(metadata.into()))).await }
        pub async fn volume(&self, volume: f64) { self.send(Command::Volume(volume)).await }
        pub async fn plugin_activated(&self, activated: bool) {
            if !activated && self.config.exit_with_plugin {
                self.exit().await
            } else {
                self.attach_as(activated).await
            }
        }

        pub async fn attach_as(&self, attached: bool) {
            self.send(Command::Attached(attached)).await;
            // send an update signal as well 
            // a weird side effect of this is that 
            //   the listeners always update
            //   even if they're already attached
            if attached { self.update().await }
        }
        pub async fn attach_without_update(&self) { self.send(Command::Attached(true)).await; }
        pub async fn attach(&self) { self.attach_as(true).await }
        pub async fn detach(&self) { self.attach_as(false).await }

        pub async fn update_metadata(&self) { self.send(Command::UpdateMetadata).await }
        pub async fn update_playback(&self) { self.send(Command::UpdatePlayback).await }
        pub async fn update_volume(&self) { self.send(Command::UpdateVolume).await }
        pub async fn update_plugin_activation(&self) { self.send(Command::UpdatePluginActivation).await }

        pub async fn media_control_event(&self, event: MediaControlEvent) { self.send(Command::MediaControlEvent(Arc::new(event))).await }
    }
}

pub struct Messages { tx: MessageSender, rx: Receiver<Command> }

impl Messages {
    pub fn new(config: Arc<Config>) -> Messages {
        // it's important that the channel has some
        // space in its buffer so that the filesystem
        // doesn't block in filesystem::plugin_activation_changed.
        // that could create a deadlock when the daemon exits,
        // and tries to lock the listeners to detach them
        let (tx, rx) = mpsc::channel(8);
        Self { tx: MessageSender { tx, config }, rx }
    }

    /// Returns a [clone](Clone) of the [`MessageSender`]
    pub fn sender(&self) -> MessageSender { self.tx.clone() }

    /// Listens to commands until [`Command::Exit`] is sent
    pub async fn listen_until_exit(mut self, listener: &mut impl Listener, config: Arc<Config>) -> Result<()> {
        // TODO: look into JoinSet
        // let joinset = JoinSet::new();
        // while let Some(command) = self.rx.recv().await {
        //     // trace!("message recieved: {command:?}");
        //     if let Command::Exit = command { break; }
        //     let tx = self.sender();
        //     joinset.spawn(async move {
        //         command
        //             .handle(listener, &tx, config.clone()).await
        //             .unwrap_or_else(|err| {
        //                 error!("failed to handle command: {err:?}");
        //             });
        //     });
        // }
        while let Some(command) = self.rx.recv().await {
            if let Command::Exit = command { break; }
            command
                .handle(listener, &self.tx, config.clone()).await
                .unwrap_or_else(|err| {
                    error!("failed to handle command: {err:?}");
                });
        }
        Ok(())
    }
}

#[derive(Debug)]
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
