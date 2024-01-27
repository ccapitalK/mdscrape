use core::future::Future;
use std::convert::From;
use tokio::time::Duration;

const MANGADEX_RATE_LIMIT_CODE: u16 = 429;

#[derive(Debug)]
pub enum DownloadError {
    IOError(std::io::Error),
    NoSuchChapter(usize),
    ChapterIsWrongLanguage(usize),
    ParseError(url::ParseError),
    ReqwestError(reqwest::Error),
    RateLimitError(reqwest::Error),
}

pub type Result<T> = std::result::Result<T, DownloadError>;

impl From<std::io::Error> for DownloadError {
    fn from(e: std::io::Error) -> Self {
        DownloadError::IOError(e)
    }
}

impl From<url::ParseError> for DownloadError {
    fn from(e: url::ParseError) -> Self {
        DownloadError::ParseError(e)
    }
}

impl From<reqwest::Error> for DownloadError {
    fn from(e: reqwest::Error) -> Self {
        if e.status().map(|c| c.as_u16()) == Some(MANGADEX_RATE_LIMIT_CODE) {
            DownloadError::RateLimitError(e)
        } else {
            DownloadError::ReqwestError(e)
        }
    }
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::IOError(e) => write!(f, "IO error: {}", e),
            DownloadError::ParseError(e) => write!(f, "Url parsing error: {}", e),
            DownloadError::NoSuchChapter(chapter_id) => write!(f, "Chapter not found: {}", chapter_id),
            DownloadError::ChapterIsWrongLanguage(chapter_id) => {
                write!(f, "Chapter has wrong lang code: {}", chapter_id)
            }
            DownloadError::ReqwestError(e) => write!(f, "Download error: {}", e),
            DownloadError::RateLimitError(e) => write!(f, "Downloads exceeded rate limit: {}", e),
        }
    }
}

impl std::error::Error for DownloadError {}

trait MaybePermanentError {
    fn is_permanent(&self) -> bool;
}

impl MaybePermanentError for DownloadError {
    fn is_permanent(&self) -> bool {
        match self {
            DownloadError::IOError(_) => true,
            DownloadError::ParseError(_) => true,
            DownloadError::NoSuchChapter(_) => true,
            DownloadError::ChapterIsWrongLanguage(_) => true,
            DownloadError::RateLimitError(_) => false,
            DownloadError::ReqwestError(e) => e.is_builder() || e.is_status(),
        }
    }
}

pub async fn with_retry<T, F, G>(f: impl Fn() -> F, wait: impl Fn() -> G) -> Result<T>
where
    F: Future<Output = Result<T>>,
    G: Future<Output = ()>,
{
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut duration = Duration::from_millis(200);
    let mut count = 0;
    loop {
        count += 1;
        match f().await {
            v @ Ok(_) => return v,
            Err(DownloadError::RateLimitError(_)) => wait().await,
            Err(e) if e.is_permanent() => return Err(e),
            e => {
                if count < 4 {
                    tokio::time::sleep(duration.mul_f64(rng.gen())).await;
                    duration *= 3;
                } else {
                    return e;
                }
            }
        }
    }
}
