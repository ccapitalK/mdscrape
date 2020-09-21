use url::Origin;

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

use crate::throttle::{TicketFuture, Ticketer};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum DownloadType {
    Title(usize),
    Chapter(usize),
}

pub struct ScrapeContext {
    pub verbose: bool,
    pub lang_code: String,
    pub start_chapter: Option<usize>,
    pub end_chapter: Option<usize>,
    pub ignored_groups: HashSet<usize>,
    pub download_type: DownloadType,
    pub show_progress: bool,
    pub progress: Arc<indicatif::MultiProgress>,
    ticketer: RefCell<Ticketer<Origin>>,
}

impl ScrapeContext {
    pub fn from_args() -> Self {
        let mut verbose = false;
        let mut download_type_is_title = true;
        let mut resource_id = 0usize;
        let mut lang_code = "gb".to_owned();
        let mut start_chapter = None;
        let mut end_chapter = None;
        let mut print_info = false;
        let mut show_progress = true;
        let mut ignored_groups_str = String::new();
        let mut global_threshold = 1;
        let mut per_origin_threshold = 1;
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
        ScrapeContext {
            verbose,
            lang_code,
            start_chapter,
            end_chapter,
            show_progress,
            download_type: if download_type_is_title {
                DownloadType::Title(resource_id)
            } else {
                DownloadType::Chapter(resource_id)
            },
            ignored_groups: if ignored_groups_str.len() > 0 {
                ignored_groups_str
                    .split(",")
                    .map(|v| {
                        v.parse::<usize>().expect(&format!(
                            "Failed to parse ignored_group [expected integer group id]: {}",
                            v
                        ))
                    })
                    .collect()
            } else {
                Default::default()
            },
            progress: Arc::new(indicatif::MultiProgress::new()),
            ticketer: RefCell::new(Ticketer::new(per_origin_threshold, global_threshold)),
        }
    }
    pub fn get_ticket<'origin>(&'origin self, origin: &'origin Origin) -> TicketFuture<'origin, Origin> {
        TicketFuture::new(origin, &self.ticketer, false)
    }
    pub fn get_priority_ticket<'origin>(&'origin self, origin: &'origin Origin) -> TicketFuture<'origin, Origin> {
        TicketFuture::new(origin, &self.ticketer, true)
    }
}
