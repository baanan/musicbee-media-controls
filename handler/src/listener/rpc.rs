#![allow(dead_code)]

use std::{sync::Arc, collections::HashMap, path::{Path, PathBuf}, time::{Instant, Duration}};

use anyhow::{Result, anyhow, Context, bail};
use async_trait::async_trait;
use discord_rich_presence::{DiscordIpcClient, DiscordIpc, activity::{Activity, Assets}};
use futures::future::join_all;
use log::trace;
use reqwest::{multipart::{Form, Part}, Client, Body};
use serde_json::Value;
use souvlaki::MediaMetadata;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
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

#[async_trait]
impl Listener for Rpc {
    async fn metadata(&mut self, metadata: &MediaMetadata) -> Result<()> {
        if !self.attached { return Ok(()); }

        let MediaMetadata { title, album, artist, cover_url, .. } = metadata;

        let large_image = if let Some(cover_url) = cover_url {
            self.cover_cache.resolve_str(cover_url).await?.to_string()
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

    async fn volume(&mut self, _volume: f64) -> Result<()> {
        Ok(())
    }

    async fn playback(&mut self, _playback: &souvlaki::MediaPlayback) -> Result<()> {
        Ok(())
    }

    async fn attach(&mut self) -> Result<()> {
        if !self.attached {
            self.client.connect()
                .map_err(|err| anyhow!("failed to connect rpc: {err}"))?;
            self.attached = true;
        }
        Ok(())
    }

    async fn detach(&mut self) -> Result<()> {
        if self.attached {
            self.client.close()
                .map_err(|err| anyhow!("failed to disconnect rpc: {err}"))?;
            self.attached = false;
            self.cover_cache.clear().await?;
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
    pub async fn upload(&mut self, file: &Path) -> Result<Url> {
        // get the cover from cache
        if let Some(url) = self.get(file) { return Ok(url); }

        // upload the file
        let url = self.uploader.upload(file).await?;

        // add it to the cache
        self.insert(file, &url);

        trace!("cover uploaded to {url}");

        Ok(url)
    }

    /// Converts the url into a public url 
    ///
    /// This [uploads](Self::upload) the url it if it is a file
    pub async fn resolve(&mut self, url: Url) -> Result<Url> {
        if url.scheme() == "file" {
            let path = url.to_file_path()
                .map_err(|_| anyhow!("failed to convert file url to path"))?;
            self.upload(&path).await
        } else {
            Ok(url)
        }
    }

    /// Converts the url into a public url
    ///
    /// This [uploads](Self::upload) the url it if it is a file
    pub async fn resolve_str(&mut self, url: &str) -> Result<Url> {
        self.resolve(Url::parse(url)?).await
    }

    /// Clears all items in the cache and deletes the images from the web
    ///
    /// For imgur, this is great, because it ensures that the images get deleted no matter what.
    /// For litterbox, this isn't, because the cache is removed
    pub async fn clear(&mut self) -> Result<()> {
        self.cached.clear();
        self.uploader.wipe().await
    }
}

pub async fn form_file(path: &Path) -> Result<Part> {
    let file_name = path
        .file_name()
        .map(|val| val.to_string_lossy().to_string())
        .unwrap_or_default();

    let file = File::open(&path).await.context("failed to open file")?;
    let reader = Body::wrap_stream(ReaderStream::new(file));
    let part = Part::stream(reader).file_name(file_name);

    Ok(part)
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

#[async_trait]
trait UploadService {
    fn timeout(&self) -> Option<u64>;
    fn timeout_duration(&self) -> Option<Duration> {
        self.timeout().map(|hours| Duration::from_secs(hours * 60 * 60))
    }

    // TODO: async
    async fn upload(&mut self, file: &Path) -> Result<Url>;
    async fn wipe(&mut self) -> Result<()> { Ok(()) }
}

struct Litterbox;

impl Litterbox {
    const API_URL: &str = "https://litterbox.catbox.moe/resources/internals/api.php";
    const TIMEOUT: u64 = 1;
}

#[async_trait]
impl UploadService for Litterbox {
    fn timeout(&self) -> Option<u64> { Some(Self::TIMEOUT) }

    async fn upload(&mut self, file: &Path) -> Result<Url> {
        trace!("uploading cover `{}` to litterbox", file.display());

        let request = Form::new()
            .text("reqtype", "fileupload")
            .text("time", format!("{}h", Self::TIMEOUT)) // TODO: allow to be changed
            .part("fileToUpload", form_file(file).await.context("failed to open cover file to upload")?);

        let response = Client::new()
            .post(Self::API_URL)
            .multipart(request)
            .send().await.context("failed to upload file to litterbox")?
            .text().await.context("failed to get the text from the litterbox upload")?;

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

#[async_trait]
impl UploadService for Imgur {
    fn timeout(&self) -> Option<u64> { None }

    async fn upload(&mut self, file: &Path) -> Result<Url> {
        let image = ImgurImage::upload(file).await?;
        // this is a bit sloppy
        // not only does it require a clone, it also seperates the url from the image itself
        // so if the image list is cleared before it should be, the url could become invalid
        let url = image.url.clone();
        self.images.push(image);
        Ok(url)
    }

    async fn wipe(&mut self) -> Result<()> {
        let delete_all = self.images.drain(..) // take all images
            .map(|image| image.delete()); // create futures to delete them
        join_all(delete_all).await.into_iter() // join them all
            .collect::<Result<()>>()?; // and fold the results into a single one
        Ok(())
    }
}

#[derive(Debug)]
struct ImgurImage {
    url: Url,
    delete_hash: String,
}

impl ImgurImage {
    pub async fn upload(path: &Path) -> Result<Self> {
        let request = Form::new()
            .part("image", form_file(path).await.context("failed to open cover for upload")?);

        let response = Client::new()
            .post(Imgur::endpoint("upload"))
            .header("Authorization", format!("Client-ID {}", "0ce559de0c8a293"))
            .multipart(request)
            .send().await.context("failed to upload file to imgur")?
            .text().await.context("failed to get the text from the imgur upload")?;

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

    pub async fn delete(mut self) -> Result<()> {
        self.delete_inner().await
    }

    async fn delete_inner(&mut self) -> Result<()> {
        Client::new()
            .delete(Imgur::endpoint_with_data("image", &self.delete_hash))
            .header("Authorization", format!("Client-ID {}", "0ce559de0c8a293"))
            .send().await?;
        Ok(())
    }
}

impl Drop for ImgurImage {
    fn drop(&mut self) {
        futures::executor::block_on(self.delete_inner()).expect("failed to delete imgur image");
    }
}
