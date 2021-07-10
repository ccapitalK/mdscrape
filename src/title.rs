use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::PathBuf;
use uuid::Uuid;

use log::debug;

use crate::api::{chapter::ChapterResponse, manga::MangaFeedResponse};
use crate::chapter::ChapterInfo;
use crate::common::*;
use crate::context::ScrapeContext;
use crate::retry::{DownloadError, Result};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TitleData {
    chapters: Vec<ChapterResponse>,
}

impl TitleData {
    fn create_subdir_set(&self, ids: &[usize], base_path: &OsStr) -> Result<HashMap<usize, PathBuf>> {
        let mut subdir_set = HashMap::new();
        for (i, &id) in ids.iter().enumerate() {
            let dir_num = i + 1;
            let mut path = PathBuf::from(base_path);
            let chapter = self.chapters.get(id).unwrap();
            let chapter_id = chapter.data.id;
            // TODO: Do this a nice way
            let chapter_name = if let Some(ref x) = chapter.data.attributes.title {
                x.as_str()
            } else {
                ""
            };
            path.push(format!("md{:05} - {} - {}", dir_num, chapter_id, chapter_name));
            if !path.is_dir() {
                std::fs::create_dir(&path)?;
            }
            subdir_set.insert(id, path);
        }
        Ok(subdir_set)
    }

    pub async fn download_for_title(title_id: Uuid, context: &ScrapeContext) -> Result<Self> {
        use crate::retry::*;

        let mut offset = 0usize;
        let mut chapters: Vec<ChapterResponse> = Vec::new();

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
            chapters.append(&mut resp.results);
            if resp.offset + resp.limit >= resp.total {
                break;
            }
        }
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

    fn get_chapter_download_order(&self, context: &ScrapeContext) -> Result<Vec<usize>> {
        // TODO: Actually reorder things as needed
        Ok((0..self.chapters.len()).collect::<Vec<_>>())
    }

    pub async fn download_to_directory(&self, path: &impl AsRef<OsStr>, context: &ScrapeContext) -> Result<()> {
        use futures::stream::{FuturesUnordered, StreamExt};
        let chapter_ids = self.get_chapter_download_order(context)?;
        let title_bar = self.setup_title_bar(chapter_ids.len() as u64, context);
        let chapter_paths = self.create_subdir_set(&chapter_ids, path.as_ref())?;

        debug!("{:#?}", chapter_paths);

        let mut tasks = chapter_ids
            .into_iter()
            .map(|chapter_num| {
                let title_bar = &title_bar;
                let chapter_paths = &chapter_paths;
                let chapter_info = &self.chapters;
                async move {
                    let path = chapter_paths.get(&chapter_num).unwrap();
                    let chapter_response = chapter_info.get(chapter_num).unwrap();
                    let chapter = ChapterInfo::from_chapter_response(chapter_response.clone(), context).await?;
                    title_bar.println(format!("Got data for {}: {:?}", chapter_num, path));
                    title_bar.set_position(title_bar.position() + 1);
                    if context.verbose {
                        title_bar.println(format!("Chapter API response: {:#?}", chapter));
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
