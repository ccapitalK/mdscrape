mod chapter;
mod common;
mod context;
mod title;

use tokio::task;

use chapter::ChapterData;
use common::*;
use context::ScrapeContext;
use title::TitleData;

#[tokio::main(core_threads = 2)]
async fn main() -> OpaqueResult<()> {
    let context = ScrapeContext::from_args();
    let progress = context.progress.clone();
    let invis_bar = progress.add(indicatif::ProgressBar::hidden());
    let invis_bar_style = indicatif::ProgressStyle::default_bar().template("[MDScrape]");
    invis_bar.set_style(invis_bar_style);
    let scrape_task = async {
        let current_dir = std::env::current_dir()?;
        match context.download_type {
            context::DownloadType::Chapter(resource_id) => {
                invis_bar.println(format!("Downloading chapter: {}", resource_id));
                let chapter = ChapterData::download_for_chapter(resource_id, &context).await?;
                if context.verbose {
                    invis_bar.println(format!("Chapter API response: {:#?}", chapter));
                }
                chapter.download_to_directory(&current_dir, &context).await?;
            }
            context::DownloadType::Title(resource_id) => {
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
