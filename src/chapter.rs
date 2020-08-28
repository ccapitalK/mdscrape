use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use crate::common::*;
use crate::context::ScrapeContext;

async fn download_image(url: &str, context: &ScrapeContext) -> OpaqueResult<Vec<u8>> {
    use tokio::stream::StreamExt;
    let mut collected_data = Vec::new();
    // Make request
    let response = reqwest::get(url).await?;
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
    pub async fn download_for_chapter(chapter_id: usize, _: &ScrapeContext) -> OpaqueResult<Self> {
        use backoff::{future::FutureOperation as _, ExponentialBackoff};
        let url = format!("https://mangadex.org/api/?id={}&server=null&type=chapter", chapter_id);
        Ok(
            (|| async { Ok(reqwest::get(&url).await?.json::<ChapterData>().await?) })
                .retry(ExponentialBackoff::default())
                .await?,
        )
    }

    pub async fn download_to_directory(self, path: &impl AsRef<OsStr>, context: &ScrapeContext) -> OpaqueResult<()> {
        let mut path_buf = PathBuf::from(&path);
        let chapter_bar = {
            let style = indicatif::ProgressStyle::default_bar()
                .template("<{elapsed_precise}> [{bar:80.yellow/red}] {pos}/{len} images downloaded")
                .progress_chars("=>-");
            let chapter_bar = context
                .progress
                .add(indicatif::ProgressBar::new(self.page_array.len() as u64));
            chapter_bar.set_style(style);
            chapter_bar
        };
        for (i, filename) in self.page_array.iter().enumerate() {
            // update bar
            chapter_bar.set_position(i as u64);
            // Determine resource names
            let file_url = format!("{}{}/{}", self.server, self.hash, filename);
            let extension = filename.split(".").last().unwrap_or("png");
            path_buf.push(format!("{:04}.{}", (i + 1), extension));
            {
                use backoff::{future::FutureOperation as _, ExponentialBackoff};
                let path = path_buf.as_path();
                if path.exists() {
                    if context.verbose {
                        chapter_bar.println(format!("Skipping {:#?}, since it already exists", path));
                    }
                } else {
                    chapter_bar.println(format!("Getting {} as {:#?}", file_url, path));
                    let chapter_data = (|| async { Ok(download_image(&file_url, context).await?) })
                        .retry(ExponentialBackoff::default())
                        .await?;
                    // Create output file
                    let mut out_file = File::create(path)?;
                    // Write data
                    out_file.write(&chapter_data)?;
                }
            }
            // Remove trailing path element (the filename we just added)
            path_buf.pop();
        }
        chapter_bar.finish_and_clear();
        Ok(())
    }
}
