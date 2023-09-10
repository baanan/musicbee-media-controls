use std::sync::Arc;
use std::time::Duration;

use souvlaki::{MediaMetadata, MediaPlayback, MediaControlEvent};
use tokio::sync::broadcast::{self, Sender, Receiver};

use crate::{listener::List, config::Config};

#[derive(Debug, Clone)]
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

#[derive(Clone)]
pub struct MessageSender {
    tx: Sender<Command>,
    config: Arc<Config>,
}

impl MessageSender {
    fn send(&self, command: Command) {
        self.tx.send(command)
            .expect("message reciever hung up before program ended");
    }

    pub fn exit(&self) {
        self.detach();
        self.send(Command::Exit)
    }

    pub fn update(&self) {
        self.send(Command::Update)
    }

    pub fn playback(&self, playback: MediaPlayback) {
        if self.config.detach_on_stop {
            match playback {
                MediaPlayback::Stopped => self.detach(),
                // the attach can't also update, or it could create an infinite loop of
                // checking the playback and updating
                // FIX: the plugin doesn't know whether the listeners detached due to a
                // stop or due to user input. This could lead to unwanted attaches.
                _ => self.attach_without_update(),
            }
        }
        self.send(Command::Playback(Arc::new(playback)))
    }

    pub fn metadata(&self, metadata: MediaMetadata<'_>) {
        self.send(Command::Metadata(Arc::new(metadata.into())))
    }

    pub fn volume(&self, volume: f64) {
        self.send(Command::Volume(volume))
    }

    pub fn plugin_activated(&self, activated: bool) {
        if !activated && self.config.exit_with_plugin {
            self.exit()
        } else {
            self.attach_as(activated)
        }
    }

    pub fn attach_as(&self, attached: bool) {
        self.send(Command::Attached(attached));
        // send an update signal as well 
        // a weird side effect of this is that 
        //   the listeners always update
        //   even if they're already attached
        if attached { self.update() }
    }

    pub fn attach_without_update(&self) { self.send(Command::Attached(true)); }
    pub fn attach(&self) { self.attach_as(true) }
    pub fn detach(&self) { self.attach_as(false) }

    pub fn update_metadata(&self) { self.send(Command::UpdateMetadata) }
    pub fn update_playback(&self) { self.send(Command::UpdatePlayback) }
    pub fn update_volume(&self) { self.send(Command::UpdateVolume) }
    pub fn update_plugin_activation(&self) { self.send(Command::UpdatePluginActivation) }

    pub fn media_control_event(&self, event: MediaControlEvent) { self.send(Command::MediaControlEvent(Arc::new(event))) }
}

pub struct Messages { tx: MessageSender, rx: Receiver<Command> }

impl Messages {
    pub fn new(config: Arc<Config>) -> Self {
        // it's important that the channel has some
        // space in its buffer so that the filesystem
        // doesn't block in filesystem::plugin_activation_changed.
        // that could create a deadlock when the daemon exits,
        // and tries to lock the listeners to detach them
        let (tx, rx) = broadcast::channel(8);
        Self { tx: MessageSender { tx, config }, rx }
    }

    /// Returns a [clone](Clone) of the [`MessageSender`]
    pub fn sender(&self) -> MessageSender { self.tx.clone() }

    /// Listens to commands until [`Command::Exit`] is sent
    pub async fn listen_until_exit(self, list: List, config: Arc<Config>) {
        list.listen(self.rx, &config).await
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
        let Self { title, album, artist, cover_url, duration } = self;
        MediaMetadata {
            title: title.as_deref(),
            album: album.as_deref(),
            artist: artist.as_deref(),
            cover_url: cover_url.as_deref(),
            duration: *duration,
        }
    }
}

// this is ugly because souvlaki turns a MediaMetadata into its own form of OwnedMetadata
// this means that ToOwned gets called twice, but I can't think of another way to do it
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
