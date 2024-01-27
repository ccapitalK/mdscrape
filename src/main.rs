#![forbid(unsafe_code)]

mod api;
mod chapter;
mod client;
mod common;
mod context;
mod retry;
mod throttle;
mod throttle2;
mod title;
mod tui;

use tokio::task;

use log::{info, LevelFilter};

use simple_logger::SimpleLogger;

use chapter::ChapterInfo;
use common::*;
use context::ScrapeContext;
use title::TitleData;

#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[tokio::main(worker_threads = 1)]
async fn main() -> OpaqueResult<()> {
    SimpleLogger::new().with_level(LevelFilter::Warn).init().unwrap();
    // let tui = TUI::new()?;
    let context = ScrapeContext::from_args();
    println!("{:?}", context);
    // Setup progress bar
    let progress = context.progress.clone();
    let invis_bar = progress.add(indicatif::ProgressBar::hidden());
    let invis_bar_style = indicatif::ProgressStyle::default_bar().template("[MDScrape]");
    invis_bar.set_style(invis_bar_style);

    let scrape_task = async {
        let current_dir = std::env::current_dir()?;
        match context.download_type {
            context::DownloadType::Chapter(ref uuid) => {
                info!("Going to download chapter {:?}", uuid);
                let chapter = ChapterInfo::download_for_chapter(*uuid, &context).await?;
                if context.verbose {
                    info!("Got chapter information: {:#?}", chapter);
                }
                chapter.download_to_directory(&current_dir, &context).await?;
            }
            context::DownloadType::Title(ref uuid) => {
                info!("Downloading title: {}", uuid);
                let title = TitleData::download_for_title(*uuid, &context).await?;
                if context.verbose {
                    info!("Title API response: {:#?}", title);
                }
                title.download_to_directory(&current_dir, &context).await?;
            }
        }
        invis_bar.finish_and_clear();
        Ok(())
    };
    if context.show_progress {
        let progress_res = task::spawn_blocking(move || progress.join());
        let scrape_res: OpaqueResult<_> = scrape_task.await;
        scrape_res?;
        progress_res.await??;
    } else {
        let scrape_res: OpaqueResult<_> = scrape_task.await;
        scrape_res?;
    }
    Ok(())
}
