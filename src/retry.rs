use core::future::Future;
use std::convert::From;
use tokio::time::Duration;

#[derive(Debug)]
pub enum DownloadError {
    IOError(std::io::Error),
    ReqwestError(reqwest::Error),
}

impl From<std::io::Error> for DownloadError {
    fn from(e: std::io::Error) -> Self {
        DownloadError::IOError(e)
    }
}

impl From<reqwest::Error> for DownloadError {
    fn from(e: reqwest::Error) -> Self {
        DownloadError::ReqwestError(e)
    }
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::IOError(e) => write!(f, "IO error: {}", e),
            DownloadError::ReqwestError(e) => write!(f, "Download error: {}", e),
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
            DownloadError::ReqwestError(e) => e.is_builder() || e.is_status(),
        }
    }
}

pub async fn with_retry<T, F>(f: impl Fn() -> F) -> Result<T, DownloadError>
where
    F: Future<Output = Result<T, DownloadError>>,
{
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut duration = Duration::from_millis(200);
    let mut count = 0;
    loop {
        count += 1;
        match f().await {
            v @ Ok(_) => return v,
            Err(e) if e.is_permanent() => return Err(e),
            e => {
                if count < 4 {
                    tokio::time::delay_for(duration.mul_f64(rng.gen())).await;
                    duration *= 3;
                } else {
                    return e;
                }
            }
        }
    }
}
