use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::PathBuf;
use uuid::Uuid;

use log::debug;

use crate::api::{chapter::ChapterData, manga::MangaFeedResponse};
use crate::chapter::ChapterInfo;
use crate::common::*;
use crate::context::ScrapeContext;
use crate::retry::{DownloadError, Result};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TitleData {
    chapters: Vec<ChapterData>,
}

fn sanitize_chapter_name(name: &str) -> String {
    let mut sanitized_name = String::new();
    for c in name.chars() {
        sanitized_name.push(match c {
            '\x00' => '0',
            '/' => '\\',
            v => v,
        });
    }
    sanitized_name
}

impl TitleData {
    fn create_subdir_set(&self, base_path: &OsStr) -> Result<Vec<PathBuf>> {
        let mut subdir_set = Vec::new();
        debug!("Going to setup {} paths", self.chapters.len());
        for (i, chapter) in self.chapters.iter().enumerate() {
            let dir_num = i + 1;
            let mut path = PathBuf::from(base_path);
            let chapter_id = chapter.id;
            // let chapter_name: &str = chapter.data.attributes.title.as_ref().unwrap_or("");
            let chapter_name = if let Some(ref x) = chapter.attributes.title {
                sanitize_chapter_name(x.as_str())
            } else {
                format!("")
            };
            debug!(
                "Creating pathbuf from {:?}, {:?}, {:?}, {:?}",
                path, dir_num, chapter_id, chapter_name
            );
            path.push(format!("md{:05} - {} - {}", dir_num, chapter_id, chapter_name));
            debug!("Chose path {:?}", path);
            if !path.is_dir() {
                std::fs::create_dir(&path)?;
                debug!("Created path {:?}", path);
            }
            subdir_set.push(path);
        }
        debug!("Successfully chose paths!");
        Ok(subdir_set)
    }

    pub async fn download_for_title(title_id: Uuid, context: &ScrapeContext) -> Result<Self> {
        use crate::retry::*;

        let mut offset = 0usize;
        let mut chapters: Vec<ChapterData> = Vec::new();

        loop {
            let url = Url::parse(&format!(
                "https://api.mangadex.org/manga/{}/feed?offset={}&limit=500&translatedLanguage[]={}&order[volume]=asc&order[chapter]=asc",
                title_id,
                offset,
                context.lang_code
            )).unwrap();
            debug!("Going to download manga title information from {}", url);
            let origin = url.origin();
            let _ticket = context.get_ticket(&origin).await;
            let mut resp = with_retry(|| async {
                Ok(reqwest::get(url.clone())
                    .await?
                    .error_for_status()?
                    .json::<MangaFeedResponse>()
                    .await?)
            })
            .await?;
            let num_just_added = resp.data.len();
            chapters.append(&mut resp.data);
            offset += num_just_added;
            if offset >= resp.total {
                break;
            }
        }
        debug!("Got Chapters");
        Ok(TitleData { chapters })
    }

    fn setup_title_bar(&self, length: u64, context: &ScrapeContext) -> indicatif::ProgressBar {
        let style = indicatif::ProgressStyle::default_bar()
            .template("<{elapsed_precise}> [{bar:80.yellow/red}] Downloading chapter {pos}/{len}")
            .progress_chars("=>-");
        let title_bar = context.progress.add(indicatif::ProgressBar::new(length));
        title_bar.set_style(style);
        title_bar.tick();
        title_bar
    }

    pub async fn download_to_directory(self, path: &impl AsRef<OsStr>, context: &ScrapeContext) -> Result<()> {
        use futures::stream::{FuturesUnordered, StreamExt};
        let title_bar = self.setup_title_bar(self.chapters.len() as u64, context);
        debug!("Determining chapter paths");
        let chapter_paths = self.create_subdir_set(path.as_ref())?;

        debug!("{:#?}", chapter_paths);

        let mut tasks = self
            .chapters
            .into_iter()
            .zip(chapter_paths.into_iter())
            .map(|(chapter_data, path)| {
                let title_bar = &title_bar;
                async move {
                    let chapter = ChapterInfo::from_chapter_data(chapter_data.clone(), context).await?;
                    debug!("Got data for {}: {:?}", chapter_data.id, path);
                    title_bar.set_position(title_bar.position() + 1);
                    if context.verbose {
                        debug!("Chapter API data: {:#?}", chapter);
                    }
                    chapter.download_to_directory(&path, context).await?;
                    Ok::<(), DownloadError>(())
                }
            })
            .collect::<FuturesUnordered<_>>();

        while let Some(result) = tasks.next().await {
            result?;
        }

        title_bar.finish_and_clear();
        Ok(())
    }
}
