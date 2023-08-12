#![allow(dead_code)]

use std::{sync::Arc, collections::HashMap, path::{Path, PathBuf}, time::{Instant, Duration}};

use anyhow::{Result, anyhow, Context, bail};
use discord_rich_presence::{DiscordIpcClient, DiscordIpc, activity::{Activity, Assets}};
use log::trace;
use reqwest::blocking::{multipart::Form, Client};
use serde_json::Value;
use souvlaki::MediaMetadata;
use url::Url;

use crate::config::Config;

use super::Listener;

pub struct Rpc {
    client: DiscordIpcClient,
    cover_cache: CoverCache,
    config: Arc<Config>,
    attached: bool,
}

impl Rpc {
    pub fn new(config: Arc<Config>) -> Self {
        // create a client
        // the error type of this is weird (can't be anyhow'd),            
        // and i'm not sure how it can fail, so just expect it
        let client = DiscordIpcClient::new("942300665726767144")
            .expect("failed to create discord ipc client");

        let cover_cache = CoverCache::with(&Service::Imgur);

        Self { client, config, cover_cache, attached: false }
    }
}

impl Listener for Rpc {
    fn metadata(&mut self, metadata: &MediaMetadata) -> Result<()> {
        if !self.attached { return Ok(()); }

        trace!("updating metadata in rpc");
        let MediaMetadata { title, album, artist, cover_url, .. } = metadata;

        let large_image = if let Some(cover_url) = cover_url {
            self.cover_cache.resolve_str(cover_url)?.to_string()
        } else {
            // TODO: config
            "https://www.getmusicbee.com/img/musicbee.png".to_string()
        };

        let details = format!("{} - {}", artist.unwrap_or_default(), album.unwrap_or_default());
        let activity = Activity::new()
            .state(title.unwrap_or_default())
            .details(&details)
            .assets(Assets::new().large_image(&large_image));

        self.client.set_activity(activity)
            .map_err(|err| anyhow!("failed to set rpc activity: {err}"))?;

        Ok(())
    }

    fn volume(&mut self, _volume: f64) -> Result<()> {
        Ok(())
    }

    fn playback_inner(&mut self, _playback: &souvlaki::MediaPlayback) -> Result<()> {
        Ok(())
    }

    fn attach(&mut self) -> Result<()> {
        if !self.attached {
            self.client.connect()
                .map_err(|err| anyhow!("failed to connect rpc: {err}"))?;
            self.attached = true;
        }
        Ok(())
    }

    fn detach(&mut self) -> Result<()> {
        if self.attached {
            self.client.close()
                .map_err(|err| anyhow!("failed to disconnect rpc: {err}"))?;
            self.attached = false;
            self.cover_cache.clear()?;
        }
        Ok(())
    }

    fn attached(&self) -> bool { self.attached }
}

struct UploadedFile {
    time: Instant,
    url: Url,
}

impl UploadedFile {
    pub fn new(url: Url) -> Self {
        Self { url, time: Instant::now() }
    }
}

struct CoverCache {
    cached: HashMap<PathBuf, UploadedFile>,
    uploader: Box<dyn UploadService + Send>,
}

impl CoverCache {
    pub fn with(service: &Service) -> Self {
        Self { cached: HashMap::new(), uploader: service.create() }
    }

    /// Inserts a url for file
    fn insert(&mut self, file: &Path, url: &Url) {
        let uploaded = UploadedFile::new(url.clone());
        self.cached.insert(file.to_path_buf(), uploaded);
    }

    fn get(&mut self, file: &Path) -> Option<Url> {
        // get from cache
        let Some(UploadedFile { time, url }) = self.cached.get(file) else { return None; };
        // check timeout if it exists
        if self.uploader.timeout_duration().is_some_and(|duration|
            Instant::now().duration_since(*time) > duration 
        ) { return None; }

        trace!("successfully found cover in cache");

        Some(url.clone())
    }

    /// Uploads the file to a file provider, taking it from the cache if it exists
    pub fn upload(&mut self, file: &Path) -> Result<Url> {
        // get the cover from cache
        if let Some(url) = self.get(file) { return Ok(url); }

        // upload the file
        let url = self.uploader.upload(file)?;

        // add it to the cache
        self.insert(file, &url);

        Ok(url)
    }

    /// Converts the url into a public url 
    ///
    /// This [uploads](Self::upload) the url it if it is a file
    pub fn resolve(&mut self, url: Url) -> Result<Url> {
        if url.scheme() == "file" {
            let path = url.to_file_path()
                .map_err(|_| anyhow!("failed to convert file url to path"))?;
            self.upload(&path)
        } else {
            Ok(url)
        }
    }

    /// Converts the url into a public url
    ///
    /// This [uploads](Self::upload) the url it if it is a file
    pub fn resolve_str(&mut self, url: &str) -> Result<Url> {
        self.resolve(Url::parse(url)?)
    }

    /// Clears all items in the cache and deletes the images from the web
    ///
    /// For imgur, this is great, because it ensures that the images get deleted no matter what.
    /// For litterbox, this isn't, because the cache is removed
    pub fn clear(&mut self) -> Result<()> {
        self.cached.clear();
        self.uploader.wipe()
    }
}

pub enum Service {
    Litterbox,
    Imgur,
}

impl Service {
    fn create(&self) -> Box<dyn UploadService + Send> {
        match self {
            Self::Litterbox => Box::new(Litterbox),
            Self::Imgur => Box::new(Imgur::new())
        }
    }
}

trait UploadService {
    fn timeout(&self) -> Option<u64>;
    fn timeout_duration(&self) -> Option<Duration> {
        self.timeout().map(|hours| Duration::from_secs(hours * 60 * 60))
    }

    // TODO: async
    fn upload(&mut self, file: &Path) -> Result<Url>;
    fn wipe(&mut self) -> Result<()> { Ok(()) }
}

struct Litterbox;

impl Litterbox {
    const API_URL: &str = "https://litterbox.catbox.moe/resources/internals/api.php";
    const TIMEOUT: u64 = 1;
}

impl UploadService for Litterbox {
    fn timeout(&self) -> Option<u64> { Some(Self::TIMEOUT) }

    fn upload(&mut self, file: &Path) -> Result<Url> {
        trace!("uploading cover `{}` to litterbox", file.display());

        let request = Form::new()
            .text("reqtype", "fileupload")
            .text("time", format!("{}h", Self::TIMEOUT)) // TODO: allow to be changed
            .file("fileToUpload", file)
                .context("failed to open cover file to upload to litterbox")?;

        let response = Client::new()
            .post(Self::API_URL)
            .multipart(request)
            .send().context("failed to upload file to litterbox")?
            .text().context("failed to get the text from the litterbox upload")?;

        let url = Url::parse(&response)
            .context("failed to parse url recieved from litterbox")?;

        Ok(url)
    }
}

struct Imgur {
    // list of images
    // so drop knows what to remove
    images: Vec<ImgurImage>,
}

impl Imgur {
    const API_URL: &str = "https://api.imgur.com/3/";

    pub const fn new() -> Self {
        Self { images: vec![] }
    }

    fn endpoint(endpoint: &str) -> Url {
        Url::parse(Self::API_URL).expect("api url is a valid url")
            .join(endpoint).expect("endpoint must be valid")
    }

    fn endpoint_with_data(endpoint: &str, value: &str) -> Url {
        Url::parse(Self::API_URL).expect("api url is a valid url")
            .join(&format!("{endpoint}/{value}")).expect("endpoint must be valid")
    }
}

impl UploadService for Imgur {
    fn timeout(&self) -> Option<u64> { None }

    fn upload(&mut self, file: &Path) -> Result<Url> {
        let image = ImgurImage::upload(file)?;
        // this is a bit sloppy
        // not only does it require a clone, it also seperates the url from the image itself
        // so if the image list is cleared before it should be, the url could become invalid
        let url = image.url.clone();
        self.images.push(image);
        Ok(url)
    }

    fn wipe(&mut self) -> Result<()> {
        // drops all images
        // and thus deletes them
        self.images.clear();
        Ok(())
    }
}

#[derive(Debug)]
struct ImgurImage {
    url: Url,
    delete_hash: String,
}

impl ImgurImage {
    pub fn upload(path: &Path) -> Result<Self> {
        let request = Form::new().file("image", path)
            .context("failed to open cover file to upload to imgur")?;

        let response = Client::new()
            .post(Imgur::endpoint("upload"))
            .header("Authorization", format!("Client-ID {}", "0ce559de0c8a293"))
            .multipart(request)
            .send().context("failed to upload file to imgur")?
            .text().context("failed to get the text from the imgur upload")?;

        let json: Value = serde_json::from_str(&response)
            .context("failed to parse imgur upload response")?;

        if !json["success"].as_bool().unwrap_or(false) {
            bail!("imgur upload failed: {}", json["data"]["error"]);
        }

        let url = json["data"]["link"].as_str()
            .context("failed to get link from imgur upload")?;
        let url = Url::parse(url)
            .context("failed to parse imgur upload link")?;
        let delete_hash = json["data"]["deletehash"].as_str()
            .context("failed to get delete hash for imgur upload")?
            .to_string();
        Ok(Self { url, delete_hash })
    }

    fn delete_inner(&mut self) -> Result<()> {
        Client::new()
            .delete(Imgur::endpoint_with_data("image", &self.delete_hash))
            .header("Authorization", format!("Client-ID {}", "0ce559de0c8a293"))
            .send()?;
        Ok(())
    }
}

impl Drop for ImgurImage {
    fn drop(&mut self) {
        self.delete_inner().expect("failed to delete imgur image");
    }
}
