use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::PathBuf;

use crate::chapter::ChapterData;
use crate::common::*;
use crate::context::ScrapeContext;
use crate::retry::{DownloadError, Result};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChapterReferenceData {
    timestamp: u64,
    lang_code: String,
    volume: String,
    chapter: String,
    title: String,
    group_id: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TitleData {
    chapter: HashMap<usize, ChapterReferenceData>,
}

impl TitleData {
    fn create_subdir_set(&self, ids: &[usize], base_path: &OsStr) -> Result<HashMap<usize,PathBuf>> {
        let folder_id_regex = regex::Regex::new("^([0-9]{4,}) - Vol").unwrap();
        let mut ids: HashSet<_> = ids.iter().collect();
        let mut paths = HashMap::new();
        let dir_walker = walkdir::WalkDir::new(base_path)
            .into_iter()
            .filter_map(|e| e.ok().filter(|d| d.file_type().is_dir()));
        for entry in dir_walker {
            if let Some(cap) = folder_id_regex.captures(&entry.file_name().to_string_lossy()) {
                let path_str = cap.get(1).unwrap().as_str().trim_start_matches('0');
                if let Ok(matched_id) = path_str.parse::<usize>() {
                    if ids.contains(&matched_id) {
                        ids.remove(&matched_id);
                        let mut path = PathBuf::from(base_path);
                        path.push(entry.file_name());
                        paths.insert(matched_id, path);
                    }
                }
            }
        }
        for id in ids {
            let chapter_info = &self.chapter[id];
            let mut path = PathBuf::from(base_path);
            path.push(escape_path_string(format!(
                "{:08} - Vol {} - Chapter {} - {} - {}",
                id, chapter_info.volume, chapter_info.chapter, chapter_info.lang_code, chapter_info.title
            )));
            std::fs::create_dir(&path)?;
            paths.insert(*id, path);
        }
        Ok(paths)
    }

    pub async fn download_for_title(title_id: usize, context: &ScrapeContext) -> Result<Self> {
        use crate::retry::*;
        let url = Url::parse(&format!("https://mangadex.org/api/?id={}&server=null&type=manga", title_id)).unwrap();
        let origin = url.origin();
        let _ticket = context.get_ticket(&origin).await;
        Ok(with_retry(|| async {
            Ok(reqwest::get(url.clone())
                .await?
                .error_for_status()?
                .json::<TitleData>()
                .await?)
        })
        .await?)
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
        let mut chapter_ids: Vec<_> = self
            .chapter
            .iter()
            .filter(|(_, chapter)| {
                chapter.lang_code == context.lang_code && !context.ignored_groups.contains(&chapter.group_id)
            })
            .map(|(a, _)| *a)
            .collect();
        chapter_ids.sort();
        let get_index = |chapter_id: Option<_>| {
            chapter_id
                .map(|chapter_id| {
                    chapter_ids.binary_search(&chapter_id).map_err(|_| {
                        if self.chapter.contains_key(&chapter_id) {
                            DownloadError::ChapterIsWrongLanguage(chapter_id)
                        } else {
                            DownloadError::NoSuchChapter(chapter_id)
                        }
                    })
                })
                .transpose()
        };
        let start = get_index(context.start_chapter)?.unwrap_or(0);
        let end = get_index(context.end_chapter)?.unwrap_or(chapter_ids.len());
        let mut seen = HashSet::new();
        chapter_ids = chapter_ids.drain(start..end).filter(|id| {
            let data = self.chapter.get(&id).unwrap();
            let description = (data.volume.to_string(), data.chapter.to_string());

            if !seen.contains(&description) {
                seen.insert(description);
                true
            } else {
                false
            }
        }).collect();
        Ok(chapter_ids)
    }

    pub async fn download_to_directory(&self, path: &impl AsRef<OsStr>, context: &ScrapeContext) -> Result<()> {
        use futures::stream::{FuturesUnordered, StreamExt};
        let chapter_ids = self.get_chapter_download_order(context)?;
        let title_bar = self.setup_title_bar(chapter_ids.len() as u64, context);
        let chapter_paths = self.create_subdir_set(&chapter_ids, path.as_ref())?;

        let mut tasks = chapter_ids.into_iter().map(|chapter_id| {
            let title_bar = &title_bar;
            let chapter_paths = &chapter_paths;
            async move {
                let path = chapter_paths.get(&chapter_id).unwrap();
                let chapter = ChapterData::download_for_chapter(chapter_id, context).await?;
                title_bar.println(format!("Got data for {}: {:?}", chapter_id, path));
                title_bar.set_position(title_bar.position() + 1);
                if context.verbose {
                    title_bar.println(format!("Chapter API response: {:#?}", chapter));
                }
                chapter.download_to_directory(&path, context).await?;
                Ok::<(), DownloadError>(())
            }
        }).collect::<FuturesUnordered<_>>();

        while let Some(result) = tasks.next().await {
            result?;
        }

        title_bar.finish_and_clear();
        Ok(())
    }
}
