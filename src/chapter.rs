use reqwest::Url;
use serde::{Deserialize, de::DeserializeOwned, Serialize};
use std::ffi::OsStr;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

use crate::api;

use log::debug;

use crate::context::ScrapeContext;
use crate::retry::{with_retry, DownloadError, Result};
use uuid::Uuid;

async fn download_image(url: &Url, context: &ScrapeContext) -> Result<Vec<u8>> {
    use tokio::stream::StreamExt;
    let mut collected_data = Vec::new();
    // Make request
    let response = reqwest::get(url.clone()).await?.error_for_status()?;
    // Get response size, if known so progress bar can render
    let content_length = response.content_length();
    // Get data
    let mut data_stream = response.bytes_stream();
    // Create progress bar
    let bar = context
        .progress
        .add(indicatif::ProgressBar::new(if let Some(size) = content_length {
            size
        } else {
            2
        }));
    let image_bar_style = indicatif::ProgressStyle::default_bar()
        .template("<{elapsed_precise}> [{bar:80.yellow/red}] {pos}/{len} bytes received")
        .progress_chars("=>-");
    bar.set_style(image_bar_style);
    bar.tick();
    // Show progress bar while downloading
    while let Some(data) = data_stream.next().await {
        collected_data.extend_from_slice(&data?);
        bar.set_position(if content_length.is_some() {
            collected_data.len() as u64
        } else {
            1
        });
    }
    if context.verbose {
        bar.println(format!("Finished Downloading {}", url));
    }
    Ok(collected_data)
}

#[derive(Clone, Debug)]
pub struct ChapterInfo {
    id: Uuid,
    lang_code: String,
    hash: String,
    server: String,
    page_array: Rc<Vec<String>>,
}

impl ChapterInfo {
    async fn download<T>(url: Url, context: &ScrapeContext) -> Result<T> where T: DeserializeOwned {
        let origin = url.origin();
        let _ticket = context.get_ticket(&origin).await;
        with_retry(|| async {
            Ok(reqwest::get(url.clone())
                .await?
                .error_for_status()?
                .json::<T>()
                .await?)
        }).await
    }

    pub async fn from_chapter_response(response: api::chapter::ChapterResponse, context: &ScrapeContext) -> Result<Self> {
        let md_at_home_info_url = Url::parse(&format!(
            "https://api.mangadex.org/at-home/server/{}",
            response.data.id
        )).unwrap();

        debug!("Going to determine owning server address from \"{}\"", md_at_home_info_url);
        let server_info: api::at_home::ServerInfoResponse = Self::download(md_at_home_info_url, context).await?;
        Ok(ChapterInfo {
            server: server_info.base_url,
            id: response.data.id,
            page_array: response.data.attributes.data,
            hash: response.data.attributes.hash,
            lang_code: response.data.attributes.translated_language.clone(),
        })
    }

    pub async fn download_for_chapter(chapter_id: Uuid, context: &ScrapeContext) -> Result<Self> {
        use crate::retry::*;
        let chapter_info_url = Url::parse(&format!(
            "https://api.mangadex.org/chapter/{}",
            chapter_id
        )).unwrap();

        debug!("Going to download chapter info from \"{}\"", chapter_info_url);
        let response: api::chapter::ChapterResponse = Self::download(chapter_info_url, context).await?;
        Self::from_chapter_response(response, context).await
    }

    pub async fn download_to_directory(self, path: &impl AsRef<OsStr>, context: &ScrapeContext) -> Result<()> {
        use futures::stream::{FuturesUnordered, StreamExt};
        use std::rc::Rc;
        let chapter_bar = Rc::new({
            let style = indicatif::ProgressStyle::default_bar()
                .template("<{elapsed_precise}> [{bar:80.yellow/red}] {pos}/{len} images downloaded")
                .progress_chars("=>-");
            let chapter_bar = context
                .progress
                .add(indicatif::ProgressBar::new(self.page_array.len() as u64));
            chapter_bar.set_style(style);
            chapter_bar
        });
        let url_base = format!("{}/data/{}", self.server, self.hash);
        let origin = Url::parse(&url_base)?.origin();
        context.get_ticket(&origin).await;
        debug!("Determined url_base as {}", url_base);
        let mut tasks = self
            .page_array
            .iter()
            .enumerate()
            .map(|(i, filename)| {
                let chapter_bar = chapter_bar.clone();
                let url_base = &url_base;
                let origin = &origin;
                async move {
                    debug!("Async closure called");
                    let mut path_buf = PathBuf::from(&path);
                    // Determine resource names
                    let file_url = format!("{}/{}", url_base, filename);
                    let url = Url::parse(&file_url)?;
                    let extension = filename.split(".").last().unwrap_or("png");
                    path_buf.push(format!("{:04}.{}", (i + 1), extension));
                    {
                        let path = path_buf.as_path();
                        if path.exists() {
                            if context.verbose {
                                debug!("Skipping {:#?}, since it already exists", path);
                            }
                        } else {
                            let _ticket = context.get_priority_ticket(origin).await;
                            debug!("Getting {} as {:#?}", file_url, path);
                            let chapter_data =
                                with_retry(|| async { Ok(download_image(&url, context).await?) }).await?;
                            // Create output file
                            let mut out_file = File::create(path)?;
                            // Write data
                            out_file.write(&chapter_data)?;
                        }
                    }
                    // Update bar
                    chapter_bar.set_position(chapter_bar.position() + 1);
                    Ok::<(), DownloadError>(())
                }
            })
            .collect::<FuturesUnordered<_>>();

        while let Some(result) = tasks.next().await {
            result?;
        }

        chapter_bar.finish_and_clear();
        Ok(())
    }
}
