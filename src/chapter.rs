use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use crate::context::ScrapeContext;
use crate::retry::{with_retry, DownloadError, Result};

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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChapterData {
    id: usize,
    lang_code: String,
    hash: String,
    server: String,
    page_array: Vec<String>,
    manga_id: usize,
    group_id: usize,
}

impl ChapterData {
    pub async fn download_for_chapter(chapter_id: usize, context: &ScrapeContext) -> Result<Self> {
        use crate::retry::*;
        let url = Url::parse(&format!(
            "https://mangadex.org/api/?id={}&server=null&type=chapter",
            chapter_id
        ))
        .unwrap();
        let origin = url.origin();
        let _ticket = context.get_ticket(&origin).await;
        Ok(with_retry(|| async {
            Ok(reqwest::get(url.clone())
                .await?
                .error_for_status()?
                .json::<ChapterData>()
                .await?)
        })
        .await?)
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
        let url_base = format!("{}{}", self.server, self.hash);
        let origin = Url::parse(&url_base)?.origin();
        let mut tasks = self
            .page_array
            .iter()
            .enumerate()
            .map(|(i, filename)| {
                let chapter_bar = chapter_bar.clone();
                let url_base = &url_base;
                let origin = &origin;
                async move {
                    let _ticket = context.get_ticket(origin).await;
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
                                chapter_bar.println(format!("Skipping {:#?}, since it already exists", path));
                            }
                        } else {
                            chapter_bar.println(format!("Getting {} as {:#?}", file_url, path));
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
