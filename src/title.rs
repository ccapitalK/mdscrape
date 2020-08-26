use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::PathBuf;

use crate::chapter::{ChapterData, ChapterReferenceData};
use crate::common::*;
use crate::context::ScrapeContext;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TitleData {
    chapter: HashMap<usize, ChapterReferenceData>,
}

impl TitleData {
    pub async fn download_for_title(title_id: usize, _: &ScrapeContext) -> OpaqueResult<Self> {
        let url = format!("https://mangadex.org/api/?id={}&server=null&type=manga", title_id);
        reqwest::get(&url)
            .await?
            .json::<TitleData>()
            .await
            .map_err(|x| Box::new(x).into())
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

    fn get_chapter_download_order(&self, context: &ScrapeContext) -> OpaqueResult<Vec<usize>> {
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
                            ScrapeError::ChapterIsWrongLanguage(chapter_id)
                        } else {
                            ScrapeError::NoSuchChapter(chapter_id)
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

    pub async fn download_to_directory(&self, path: &impl AsRef<OsStr>, context: &ScrapeContext) -> OpaqueResult<()> {
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let chapter_ids = self.get_chapter_download_order(context)?;
        let title_bar = self.setup_title_bar(chapter_ids.len() as u64, context);

        for (i, chapter_id) in chapter_ids.into_iter().enumerate() {
            let data = self.chapter.get(&chapter_id).unwrap();
            let mut path_buf = PathBuf::from(path);
            let canonical_name = data.get_canonical_name(chapter_id);
            title_bar.set_position(i as u64 + 1);
            path_buf.push(&canonical_name);

            {
                let path = path_buf.as_path();
                let description = (data.volume.to_string(), data.chapter.to_string());
                if !path.is_dir() {
                    std::fs::create_dir(path)?;
                    if seen.contains(&description) {
                        path_buf.pop();
                        continue;
                    }
                }
                title_bar.println(format!("Getting data for {}: \"{}\"", chapter_id, canonical_name));
                let chapter = ChapterData::download_for_chapter(chapter_id, context).await?;
                if context.verbose {
                    title_bar.println(format!("Chapter API response: {:#?}", chapter));
                }
                chapter.download_to_directory(&path, context).await?;
                seen.insert(description);
            }

            path_buf.pop();
        }

        title_bar.finish_and_clear();
        Ok(())
    }
}
