#![allow(dead_code)]

use std::{sync::Arc, collections::HashMap, path::{Path, PathBuf}, time::{Instant, Duration}};

use anyhow::{Result, anyhow, Context, bail};
use async_trait::async_trait;
use discord_rich_presence::{DiscordIpcClient, DiscordIpc, activity::{Activity, Assets}};
use futures::future::join_all;
use log::trace;
use reqwest::{multipart::{Form, Part}, Client, Body};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use souvlaki::MediaMetadata;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use url::Url;

use crate::{config::Config, messages::Command};

use super::Listener;

pub struct Rpc {
    client: DiscordIpcClient,
    cover_cache: CoverCache,
    config: Arc<Config>,
    attached: bool,
}

#[async_trait]
impl Listener for Rpc {
    async fn handle(&mut self, command: Command, _: &Config) -> Result<()> {
        match command {
            Command::Metadata(metadata) => 
                self.metadata(&(*metadata).as_ref()).await.context("failed to set metadata")?, 
            Command::Attached(true) if !self.attached =>
                self.attach().context("failed to attach")?,
            Command::Attached(false) if self.attached => 
                self.detach().await.context("failed to detach")?,
            // NOTE: ignores attaches when already attached and detaches when already detached
            _ => (),
        }
        Ok(())
    }

    fn name(&self) -> &'static str { "rpc" }
}

impl Rpc {
    pub fn new(config: Arc<Config>) -> Self {
        // create a client
        // the error type of this is weird (can't be anyhow'd),            
        // and i'm not sure how it can fail, so just expect it
        let client = DiscordIpcClient::new("942300665726767144")
            .expect("failed to create discord ipc client");

        let cover_cache = CoverCache::with(&config.rpc.service);

        Self { client, config, cover_cache, attached: false }
    }

    async fn metadata(&mut self, metadata: &MediaMetadata<'_>) -> Result<()> {
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

    fn attach(&mut self) -> Result<()> {
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
}

struct CoverCache {
    cached: HashMap<PathBuf, Box<dyn UploadedFile + Send>>,
    uploader: Box<dyn UploadService + Send>,
}

impl CoverCache {
    pub fn with(service: &Service) -> Self {
        Self { cached: HashMap::new(), uploader: service.create() }
    }

    /// Inserts a url for file
    fn insert(&mut self, file: &Path, uploaded: Box<dyn UploadedFile + Send>) {
        self.cached.insert(file.to_path_buf(), uploaded);
    }

    fn get(&mut self, file: &Path) -> Option<Url> {
        // get from cache
        let file = self.cached.get(file)?.url()?;
        trace!("successfully found cover in cache");
        Some(file)
    }

    /// Uploads the file to a file provider, taking it from the cache if it exists
    pub async fn upload(&mut self, file: &Path) -> Result<Url> {
        // get the cover from cache
        if let Some(url) = self.get(file) { return Ok(url); }

        // upload the file
        let uploaded = self.uploader.upload(file).await?;

        // get the url
        let url = uploaded.url().expect("uploaded files must have a url after being created");

        // add it to the cache
        self.insert(file, uploaded);

        trace!("cover uploaded to {url}");

        Ok(url)
    }

    /// Converts the url into a public url 
    ///
    /// This [uploads](Self::upload) the url it if it is a file
    pub async fn resolve(&mut self, url: Url) -> Result<Url> {
        if url.scheme() == "file" {
            let path = url.to_file_path()
                .map_err(|_| anyhow!("failed to convert file url '{url}' to path"))?;
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
        let images = self.cached.drain();
        if self.uploader.needs_deleting() {
            let delete_all = images.map(|(_, file)| file) // take all images
                .map(UploadedFile::delete); // create futures to delete them
            join_all(delete_all).await.into_iter() // join them all
                .collect::<Result<()>>()?; // and fold the results into a single one
        }
        Ok(())
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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Service {
    Litterbox,
    Imgur,
}

impl Service {
    fn create(&self) -> Box<dyn UploadService + Send> {
        match self {
            Self::Litterbox => Box::new(Litterbox),
            Self::Imgur => Box::new(Imgur)
        }
    }
}

#[async_trait]
trait UploadService {
    /// Uploads the file to the upload service
    async fn upload(&mut self, file: &Path) -> Result<Box<dyn UploadedFile + Send>>;
    /// Do the uploaded files from this service need to be deleted
    fn needs_deleting(&self) -> bool;
}

#[async_trait]
trait UploadedFile {
    /// Returns the url of the image or None if the image is invalid
    fn url(&self) -> Option<Url>;
    /// Deletes the image
    async fn delete(self: Box<Self>) -> Result<()>;
}

struct Litterbox;

impl Litterbox {
    const API_URL: &str = "https://litterbox.catbox.moe/resources/internals/api.php";
    const TIMEOUT: u64 = 12;
}

#[async_trait]
impl UploadService for Litterbox {
    async fn upload(&mut self, file: &Path) -> Result<Box<dyn UploadedFile + Send>> {
        Ok(Box::new(LitterboxImage::upload(file).await?))
    }

    fn needs_deleting(&self) -> bool { false }
}

struct LitterboxImage {
    url: Url,
    time: Instant,
}

#[async_trait]
impl UploadedFile for LitterboxImage {
    fn url(&self) -> Option<Url> {
        // check if the image is too old
        // technically this could break if a song starts playing near the end of the timeout,
        // but the timeout is already so large that that's probably fine
        let in_time = Instant::now().duration_since(self.time) < Duration::from_secs(Litterbox::TIMEOUT * 60 * 60);
        in_time.then(|| self.url.clone())
    }

    async fn delete(self: Box<Self>) -> Result<()> { Ok(()) }
}

impl LitterboxImage {
    async fn upload(file: &Path) -> Result<Self> {
        let request = Form::new()
            .text("reqtype", "fileupload")
            .text("time", format!("{}h", Litterbox::TIMEOUT)) // TODO: allow to be changed
            .part("fileToUpload", form_file(file).await
                .context(format!("failed to open cover file '{}' to upload", file.display()))?);

        let response = Client::new()
            .post(Litterbox::API_URL)
            .multipart(request)
            .send().await.context(format!("failed to upload file '{}' to litterbox", file.display()))?
            .text().await.context("failed to get the text from the litterbox upload")?;

        let url = Url::parse(&response)
            .context(format!("failed to parse url recieved from litterbox: {response}"))?;

        Ok(Self { url, time: Instant::now() })
    }
}

struct Imgur;

impl Imgur {
    const API_URL: &str = "https://api.imgur.com/3/";

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
    async fn upload(&mut self, file: &Path) -> Result<Box<dyn UploadedFile + Send>> {
        Ok(Box::new(ImgurImage::upload(file).await?))
    }

    fn needs_deleting(&self) -> bool { true }
}

#[derive(Debug)]
struct ImgurImage {
    // Images are only deleted through UploadedFile::delete which takes ownership of the image and
    // thus invalidates the url, or through Drop which is only run when the entire image is deleted
    // So the url doesn't have to be in an Option
    url: Url,
    // but the delete hash does to make sure Drop doesn't try to delete the image a second time
    // (although now that drop isn't being used anymore, it's not that important)
    delete_hash: Option<String>,
}

#[async_trait]
impl UploadedFile for ImgurImage {
    fn url(&self) -> Option<Url> {
        Some(self.url.clone())
    }

    async fn delete(mut self: Box<Self>) -> Result<()> {
        self.delete_inner().await?;
        Ok(())
    }
}

impl ImgurImage {
    pub async fn upload(path: &Path) -> Result<Self> {
        let request = Form::new()
            .part("image", form_file(path).await.context("failed to open cover for upload")?);

        let response = Client::new()
            .post(Imgur::endpoint("upload"))
            .header("Authorization", format!("Client-ID {}", "0ce559de0c8a293"))
            .multipart(request)
            .send().await.context(format!("failed to upload file '{}' to imgur", path.display()))?
            .text().await.context("failed to get the text from the imgur upload")?;

        let json: Value = serde_json::from_str(&response)
            .context(format!("failed to parse imgur upload response: {response}"))?;

        if !json["success"].as_bool().unwrap_or(false) {
            bail!("imgur upload failed: {}", json["data"]["error"]);
        }

        let url = json["data"]["link"].as_str()
            .context("failed to get link from imgur upload, json malformed")?;
        let url = Url::parse(url)
            .context(format!("failed to parse imgur upload link '{url}'"))?;
        let delete_hash = json["data"]["deletehash"].as_str()
            .context("failed to get delete hash for imgur upload")?
            .to_string();
        let delete_hash = Some(delete_hash);
        Ok(Self { url, delete_hash })
    }

    async fn delete_inner(&mut self) -> Result<()> {
        let Some(ref delete_hash) = self.delete_hash else { return Ok(()); };
        trace!("deleting {}", self.url);
        Client::new()
            .delete(Imgur::endpoint_with_data("image", delete_hash))
            .header("Authorization", format!("Client-ID {}", "0ce559de0c8a293"))
            .send().await.context("failed to delete imgur image")?;
        Ok(())
    }
}

// impl Drop for ImgurImage {
//     fn drop(&mut self) {
//         futures::executor::block_on(self.delete_inner()).expect("failed to delete imgur image");
//     }
// }
