use url::Origin;

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

use uuid::Uuid;

use crate::{
    retry::{self, DownloadError},
    throttle::{Ticket, TicketPolicy, Ticketer},
};

// TODO: Support lookups for old id format
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum DownloadType {
    Title(Uuid),
    Chapter(Uuid),
}

#[derive(Debug)]
pub struct ScrapeContext {
    pub verbose: bool,
    pub lang_code: String,
    pub start_chapter: Option<usize>,
    pub end_chapter: Option<usize>,
    pub ignored_groups: HashSet<usize>,
    pub download_type: DownloadType,
    pub show_progress: bool,
    pub progress: Arc<indicatif::MultiProgress>,
    ticketer: Ticketer<Origin>,
}

impl ScrapeContext {
    pub fn from_args() -> Self {
        let mut verbose = false;
        let mut download_type_is_title = true;
        let mut resource_id = String::new();
        let mut lang_code = "en".to_owned();
        let mut start_chapter = None;
        let mut end_chapter = None;
        let mut print_info = false;
        let mut show_progress = true;
        let mut ignored_groups_str = String::new();
        let mut global_threshold = 1;
        let mut per_origin_threshold = 1;
        let mut wait_time = 150_000.0f64;
        {
            use argparse::{ArgumentParser, Store, StoreFalse, StoreOption, StoreTrue};
            let mut parser = ArgumentParser::new();
            parser.set_description("Scraper for mangadex.org");
            parser
                .refer(&mut verbose)
                .add_option(&["-v", "--verbose"], StoreTrue, "Be verbose");
            parser
                .refer(&mut show_progress)
                .add_option(&["--no-progress"], StoreFalse, "Don't report progress");
            parser
                .refer(&mut download_type_is_title)
                .add_option(&["-c", "--chapter"], StoreFalse, "Download a single manga chapter")
                .add_option(&["-t", "--title"], StoreTrue, "Download an entire manga title")
                .required();
            parser.refer(&mut lang_code).add_option(
                &["-l", "--lang-code"],
                Store,
                "The language code, defaults to en (English)",
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
            parser.refer(&mut global_threshold).add_option(
                &["-g", "--global-threshold"],
                Store,
                "Max number of simultaneous connections",
            );
            parser.refer(&mut per_origin_threshold).add_option(
                &["-p", "--per-origin-threshold"],
                Store,
                "Max number of simultaneous connections per origin",
            );
            parser.refer(&mut wait_time).add_option(
                &["-w", "--rate-limit-wait-time"],
                Store,
                "Time to wait (in seconds) after being rate limited",
            );
            parser
                .refer(&mut resource_id)
                .add_argument("resource id", Store, "The resource id (the number in the URL)")
                .required();
            parser.refer(&mut ignored_groups_str).add_option(
                &["--ignored-groups"],
                Store,
                "Groups not to download chapters from, separated by commas",
            );
            parser.parse_args_or_exit();
        }
        let wait_seconds = (wait_time / 1000.0) as u64;
        let wait_nsec = (wait_time % 1000.0) as u32 * 1_000_000;
        let policy = TicketPolicy {
            max_global: global_threshold,
            max_per_site: per_origin_threshold,
            rate_limit_wait_time: tokio::time::Duration::new(wait_seconds, wait_nsec),
        };
        ScrapeContext {
            verbose,
            lang_code,
            start_chapter,
            end_chapter,
            show_progress,
            download_type: if download_type_is_title {
                DownloadType::Title(Uuid::parse_str(&resource_id).expect("Failed to parse title UUID"))
            } else {
                DownloadType::Chapter(Uuid::parse_str(&resource_id).expect("Failed to parse chapter UUID"))
            },
            ignored_groups: if !ignored_groups_str.is_empty() {
                ignored_groups_str
                    .split(',')
                    .map(|v| {
                        v.parse::<usize>().unwrap_or_else(|_| {
                            panic!("Failed to parse ignored_group [expected integer group id]: {}", v)
                        })
                    })
                    .collect()
            } else {
                Default::default()
            },
            progress: Arc::new(indicatif::MultiProgress::new()),
            ticketer: Ticketer::new(&policy),
        }
    }

    pub async fn get_ticket(&self, origin: &Origin) -> Ticket {
        self.ticketer.get_ticket(origin).await
    }

    pub async fn with_retry_for_origin<T, F>(&self, origin: &Origin, f: impl Fn() -> F) -> Result<T, DownloadError>
    where
        F: futures::Future<Output = Result<T, DownloadError>>,
    {
        log::info!("With retry for origin {:?}", origin);
        let ticket = RefCell::new(Some(self.get_ticket(origin).await));
        retry::with_retry(f, || async {
            self.ticketer.mark_origin_locked(origin);
            // Reacquire the ticket
            ticket.replace(None);
            ticket.replace(Some(self.get_ticket(origin).await));
        })
        .await
    }
}
