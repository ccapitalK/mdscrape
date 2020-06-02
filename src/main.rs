use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::ffi::OsStr;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

type OpaqueError = Box<dyn std::error::Error>;
type OpaqueResult<T> = Result<T, OpaqueError>;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TitleData {
    chapter: HashMap<usize, ChapterReferenceData>
}

impl TitleData {
    async fn download_for_title(title_id: usize) -> OpaqueResult<Self> {
        let url = format!("https://mangadex.org/api/?id={}&server=null&type=manga", title_id);
        Ok(reqwest::get(&url)
            .await?
            .json::<TitleData>()
            .await?)
    }
    async fn download_to_directory(self, path: &impl AsRef<OsStr>, lang_code: &str) -> OpaqueResult<()> {
        let mut seen: HashSet<(String, String)> = HashSet::new();
        // iterate over chapter_ids in ascending order, roughly maps to start -> finish order
        let mut chapter_ids: Vec<_> = self.chapter.iter().map(|(a, _)| a).collect();
        chapter_ids.sort();
        let chapter_ids = chapter_ids;
        for &chapter_id in chapter_ids {
            let data = self.chapter.get(&chapter_id).unwrap();
            if data.lang_code == lang_code {
                let mut path_buf = PathBuf::from(path);
                let canonical_name = data.get_canonical_name(chapter_id);
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
                    println!("Getting data for {}: \"{}\"", chapter_id, canonical_name);
                    let chapter = ChapterData::download_for_chapter(chapter_id).await?;
                    println!("Chapter API response: {:?}", chapter);
                    chapter.download_to_directory(&path).await?;
                    seen.insert(description);
                }
                path_buf.pop();
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ChapterReferenceData {
    timestamp: u64,
    lang_code: String,
    volume: String,
    chapter: String,
    title: String
}

impl ChapterReferenceData {
    fn get_canonical_name(&self, id: usize) -> String {
        format!("{:07} - Vol {} - Chapter {} - {} - {}",
            id,
            self.volume,
            self.chapter,
            self.lang_code,
            self.title)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ChapterData {
    id: usize,
    lang_code: String,
    hash: String,
    server: String,
    page_array: Vec<String>,
}

impl ChapterData {
    async fn download_for_chapter(chapter_id: usize) -> OpaqueResult<Self> {
        let url = format!("https://mangadex.org/api/?id={}&server=null&type=chapter", chapter_id);
        Ok(reqwest::get(&url)
            .await?
            .json::<ChapterData>()
            .await?)
    }

    async fn download_to_directory(self, path: &impl AsRef<OsStr>) -> OpaqueResult<()> {
        use tokio::stream::StreamExt;
        let mut path_buf = PathBuf::from(&path);
        for (i, filename) in self.page_array.iter().enumerate() {
            // Determine resource names
            let file_url = format!("{}{}/{}", self.server, self.hash, filename);
            let extension = filename.split(".").last().unwrap_or("png");
            path_buf.push(format!("{:04}.{}", (i+1), extension));
            {
                let path = path_buf.as_path();
                println!("Getting {} as {:?}", file_url, path);
                if path.exists() {
                    println!("Skipping {:?}, since it already exists", path);
                } else {
                    // Create output file
                    let mut out_file = File::create(path)?;
                    // Get data
                    let mut data_stream = reqwest::get(&file_url)
                        .await?
                        .bytes_stream();
                    // Write data
                    while let Some(data) = data_stream.next().await {
                        out_file.write(&data?)?;
                    }
                }
            }
            // Remove trailing path element (the filename we just added)
            path_buf.pop();
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum DownloadType {
    Title,
    Chapter,
}

#[tokio::main]
async fn main() -> OpaqueResult<()> {
    let mut verbose = false;
    let mut download_type = DownloadType::Title;
    let mut resource_id = 0usize;
    let mut lang_code = "gb".to_owned();
    {
        use argparse::{ArgumentParser, Store, StoreConst, StoreTrue};
        let mut parser = ArgumentParser::new();
        parser.set_description("Scraper for mangadex.org");
        parser.refer(&mut verbose)
            .add_option(&["-v", "--verbose"], StoreTrue, "Be verbose");
        parser.refer(&mut download_type)
            .add_option(&["-c", "--chapter"], StoreConst(DownloadType::Chapter), "Download a single manga chapter")
            .add_option(&["-t", "--title"], StoreConst(DownloadType::Title), "Download an entire manga title")
            .required();
        parser.refer(&mut lang_code)
            .add_option(&["-l", "--lang-code"], Store, "The language code");
        parser.refer(&mut resource_id)
            .add_argument("resource id", Store, "The resource id (the number in the URL)")
            .required();
        parser.parse_args_or_exit();
    }
    match download_type {
        DownloadType::Chapter => {
            if verbose {
                println!("Downloading chapter: {}", resource_id);
            }
            let chapter = ChapterData::download_for_chapter(resource_id).await?;
            if verbose {
                println!("Chapter API response: {:?}", chapter);
            }
            chapter.download_to_directory(&"./").await?;
        },
        DownloadType::Title => {
            if verbose {
                println!("Downloading title: {}", resource_id);
            }
            let title = TitleData::download_for_title(resource_id).await?;
            if verbose {
                println!("Title API response: {:?}", title);
            }
            title.download_to_directory(&"./", &lang_code).await?;
        },
    }
    Ok(())
}
