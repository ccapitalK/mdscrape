pub type OpaqueError = Box<dyn std::error::Error>;
pub type OpaqueResult<T> = Result<T, OpaqueError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrapeError {
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
