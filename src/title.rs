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

impl ChapterReferenceData {
    fn create_subdir_in(&self, id: usize, path: &OsStr) -> Result<PathBuf> {
        let path_prefix = format!("{:07} - Vol ", id);
        let dir_walker = walkdir::WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok().filter(|d| d.file_type().is_dir()));
        for entry in dir_walker {
            if entry.file_name().to_string_lossy().starts_with(&path_prefix) {
                return Ok(entry.into_path());
            }
        }
        let mut path = PathBuf::from(path);
        path.push(escape_path_string(format!(
            "{:07} - Vol {} - Chapter {} - {} - {}",
            id, self.volume, self.chapter, self.lang_code, self.title
        )));
        std::fs::create_dir(&path)?;
        Ok(path)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TitleData {
    chapter: HashMap<usize, ChapterReferenceData>,
}

impl TitleData {
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
        chapter_ids = chapter_ids.drain(start..end).collect();
        Ok(chapter_ids)
    }

    pub async fn download_to_directory(&self, path: &impl AsRef<OsStr>, context: &ScrapeContext) -> Result<()> {
        use futures::stream::{FuturesUnordered, StreamExt};
        use std::cell::RefCell;
        use std::rc::Rc;
        let seen: RefCell<HashSet<(String, String)>> = Default::default();
        let chapter_ids = self.get_chapter_download_order(context)?;
        let title_bar = Rc::new(self.setup_title_bar(chapter_ids.len() as u64, context));

        let mut tasks = chapter_ids.into_iter().map(|chapter_id| {
            let title_bar = title_bar.clone();
            let seen = &seen;
            async move {
                let data = self.chapter.get(&chapter_id).unwrap();
                let description = (data.volume.to_string(), data.chapter.to_string());

                if !seen.borrow().contains(&description) {
                    seen.borrow_mut().insert(description);
                    let path = data.create_subdir_in(chapter_id, path.as_ref())?;
                    let chapter = ChapterData::download_for_chapter(chapter_id, context).await?;
                    title_bar.println(format!("Got data for {}: {:?}", chapter_id, path));
                    title_bar.set_position(title_bar.position() + 1);
                    if context.verbose {
                        title_bar.println(format!("Chapter API response: {:#?}", chapter));
                    }
                    chapter.download_to_directory(&path, context).await?;
                } else {
                    title_bar.set_position(title_bar.position() + 1);
                }
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
