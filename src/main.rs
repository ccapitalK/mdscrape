use indicatif;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task;

type OpaqueError = Box<dyn std::error::Error>;
type OpaqueResult<T> = Result<T, OpaqueError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrapeError {
    NoSuchChapter(usize),
    ChapterIsWrongLanguage(usize),
}

impl std::fmt::Display for ScrapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScrapeError::NoSuchChapter(chapter_id) => write!(f, "Chapter not found: {}", chapter_id),
            ScrapeError::ChapterIsWrongLanguage(chapter_id) => write!(f, "Chapter has wrong lang code: {}", chapter_id),
        }
    }
}

impl std::error::Error for ScrapeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

struct ScrapeContext {
    verbose: bool,
    lang_code: String,
    start_chapter: Option<usize>,
    end_chapter: Option<usize>,
    ignored_groups: HashSet<usize>,
    progress: Arc<indicatif::MultiProgress>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TitleData {
    chapter: HashMap<usize, ChapterReferenceData>,
}

impl TitleData {
    async fn download_for_title(title_id: usize, _: &ScrapeContext) -> OpaqueResult<Self> {
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

    async fn download_to_directory(&self, path: &impl AsRef<OsStr>, context: &ScrapeContext) -> OpaqueResult<()> {
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

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ChapterReferenceData {
    timestamp: u64,
    lang_code: String,
    volume: String,
    chapter: String,
    title: String,
    group_id: usize,
}

impl ChapterReferenceData {
    fn get_canonical_name(&self, id: usize) -> String {
        format!(
            "{:07} - Vol {} - Chapter {} - {} - {}",
            id, self.volume, self.chapter, self.lang_code, self.title
        )
    }
}

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
struct ChapterData {
    id: usize,
    lang_code: String,
    hash: String,
    server: String,
    page_array: Vec<String>,
    manga_id: usize,
    group_id: usize,
}

impl ChapterData {
    async fn download_for_chapter(chapter_id: usize, _: &ScrapeContext) -> OpaqueResult<Self> {
        let url = format!("https://mangadex.org/api/?id={}&server=null&type=chapter", chapter_id);
        Ok(reqwest::get(&url).await?.json::<ChapterData>().await?)
    }

    async fn download_to_directory(self, path: &impl AsRef<OsStr>, context: &ScrapeContext) -> OpaqueResult<()> {
        let mut path_buf = PathBuf::from(&path);
        let chapter_bar = {
            let style = indicatif::ProgressStyle::default_bar()
                .template("<{elapsed_precise}> [{bar:80.yellow/red}] Downloading image {pos}/{len}")
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
                let path = path_buf.as_path();
                if path.exists() {
                    if context.verbose {
                        chapter_bar.println(format!("Skipping {:#?}, since it already exists", path));
                    }
                } else {
                    chapter_bar.println(format!("Getting {} as {:#?}", file_url, path));
                    let chapter_data = download_image(&file_url, context).await?;
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

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum DownloadType {
    Title,
    Chapter,
}

#[tokio::main(core_threads = 2)]
async fn main() -> OpaqueResult<()> {
    let mut verbose = false;
    let mut download_type = DownloadType::Title;
    let mut resource_id = 0usize;
    let mut lang_code = "gb".to_owned();
    let mut start_chapter = None;
    let mut end_chapter = None;
    let mut print_info = false;
    let mut no_progress = false;
    let mut ignored_groups_str = String::new();
    {
        use argparse::{ArgumentParser, Store, StoreConst, StoreOption, StoreTrue};
        let mut parser = ArgumentParser::new();
        parser.set_description("Scraper for mangadex.org");
        parser
            .refer(&mut verbose)
            .add_option(&["-v", "--verbose"], StoreTrue, "Be verbose");
        parser
            .refer(&mut no_progress)
            .add_option(&["--no-progress"], StoreTrue, "Don't report progress");
        parser
            .refer(&mut download_type)
            .add_option(
                &["-c", "--chapter"],
                StoreConst(DownloadType::Chapter),
                "Download a single manga chapter",
            )
            .add_option(
                &["-t", "--title"],
                StoreConst(DownloadType::Title),
                "Download an entire manga title",
            )
            .required();
        parser.refer(&mut lang_code).add_option(
            &["-l", "--lang-code"],
            Store,
            "The language code, defaults to gb (Great Britain/English)",
        );
        parser.refer(&mut start_chapter).add_option(
            &["-s", "--start-chapter"],
            StoreOption,
            "First chapter to download for a title",
        );
        parser.refer(&mut end_chapter).add_option(
            &["-e", "--end-chapter"],
            StoreOption,
            "Last chapter to download for a title",
        );
        parser
            .refer(&mut print_info)
            .add_option(&["-i", "--info"], StoreTrue, "Only print info about the chapter");
        parser
            .refer(&mut resource_id)
            .add_argument("resource id", Store, "The resource id (the number in the URL)")
            .required();
        parser
            .refer(&mut ignored_groups_str)
            .add_option(&["--ignored-groups"], Store, "Groups not to download chapters from, separated by commas");
        parser.parse_args_or_exit();
    }
    let progress = Arc::new(indicatif::MultiProgress::new());
    let context = ScrapeContext {
        verbose,
        lang_code,
        start_chapter,
        end_chapter,
        ignored_groups: if ignored_groups_str.len() > 0 {
            ignored_groups_str.split(",").map(|v| str::parse::<usize>(v).unwrap()).collect()
        } else {
            Default::default()
        },
        progress: progress.clone(),
    };
    let invis_bar = progress.add(indicatif::ProgressBar::hidden());
    let invis_bar_style = indicatif::ProgressStyle::default_bar().template("[MDScrape]");
    invis_bar.set_style(invis_bar_style);
    let scrape_task = async {
        let current_dir = std::env::current_dir()?;
        match download_type {
            DownloadType::Chapter => {
                invis_bar.println(format!("Downloading chapter: {}", resource_id));
                let chapter = ChapterData::download_for_chapter(resource_id, &context).await?;
                if context.verbose {
                    invis_bar.println(format!("Chapter API response: {:#?}", chapter));
                }
                chapter.download_to_directory(&current_dir, &context).await?;
            }
            DownloadType::Title => {
                invis_bar.println(format!("Downloading title: {}", resource_id));
                let title = TitleData::download_for_title(resource_id, &context).await?;
                if context.verbose {
                    invis_bar.println(format!("Title API response: {:#?}", title));
                }
                title.download_to_directory(&current_dir, &context).await?;
            }
        }
        invis_bar.finish();
        Ok(())
    };
    if no_progress {
        let scrape_res: OpaqueResult<_> = scrape_task.await;
        scrape_res?;
    } else {
        let progress_res = task::spawn_blocking(move || progress.join());
        let scrape_res: OpaqueResult<_> = scrape_task.await;
        scrape_res?;
        progress_res.await??;
    }
    Ok(())
}
