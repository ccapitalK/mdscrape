#![forbid(unsafe_code)]

mod api;
mod chapter;
mod common;
mod context;
mod retry;
mod throttle;
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

#[tokio::main(core_threads = 1)]
async fn main() -> OpaqueResult<()> {
    SimpleLogger::new().with_level(LevelFilter::Debug).init().unwrap();
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
        if let context::DownloadType::Chapter(ref uuid) = context.download_type {
            info!("Going to download chapter {:?}", uuid);
            let chapter = ChapterInfo::download_for_chapter(*uuid, &context).await?;
            if context.verbose {
                // invis_bar.println(format!("Chapter API response: {:#?}", chapter));
                info!("Got chapter information: {:#?}", chapter);
            }
            chapter.download_to_directory(&current_dir, &context).await?;
        }
        invis_bar.finish_and_clear();
        Ok(())
    };
    // let scrape_task = async {
    //     match context.download_type {
    //         context::DownloadType::Chapter(resource_id) => {
    //             invis_bar.println(format!("Downloading chapter: {}", resource_id));
    //             let chapter = ChapterData::download_for_chapter(resource_id, &context).await?;
    //             if context.verbose {
    //                 invis_bar.println(format!("Chapter API response: {:#?}", chapter));
    //             }
    //             chapter.download_to_directory(&current_dir, &context).await?;
    //         }
    //         context::DownloadType::Title(resource_id) => {
    //             invis_bar.println(format!("Downloading title: {}", resource_id));
    //             let title = TitleData::download_for_title(resource_id, &context).await?;
    //             if context.verbose {
    //                 invis_bar.println(format!("Title API response: {:#?}", title));
    //             }
    //             title.download_to_directory(&current_dir, &context).await?;
    //         }
    //     }
    //     invis_bar.finish_and_clear();
    //     Ok(())
    // };
    if /*context.show_progress*/false {
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
