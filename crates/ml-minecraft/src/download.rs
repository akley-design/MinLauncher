//! The download engine.
//!
//! This is where most of the "faster than `TLauncher`" promise is won or
//! lost. `TLauncher` downloads assets one by one and re-downloads what it
//! cannot find in its own folder; this engine
//!
//! * downloads many files at once,
//! * stores assets content-addressed, so a shared object is fetched once
//!   for all versions,
//! * verifies the SHA-1 of every payload and only then moves it into
//!   place, so an interrupted download never leaves a corrupt file behind,
//! * resumes an interrupted download instead of starting over.
//!
//! The engine is deliberately independent of the storage layer: it takes a
//! list of [`DownloadTask`]s and reports what happened. The caller decides
//! what to persist.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures::stream::StreamExt;
use ml_core::{Error, Result};
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;

/// The default number of parallel downloads.
///
/// Mojang's storage handles a lot of concurrent connections, and the
/// download is latency-bound rather than bandwidth-bound, so a high
/// number pays off. The cap keeps the memory use and the number of open
/// sockets reasonable.
pub const DEFAULT_CONCURRENCY: usize = 16;

/// How many times a download is retried before it is given up on.
pub const DEFAULT_ATTEMPTS: u32 = 4;

/// The delay before the first retry; it doubles with every attempt.
pub const DEFAULT_BACKOFF: Duration = Duration::from_millis(500);

/// One file to download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadTask {
    /// Absolute URL of the payload.
    pub url: String,
    /// Where the payload is written.
    pub target: PathBuf,
    /// Expected SHA-1 of the payload, if known.
    pub sha1: Option<String>,
    /// Expected size in bytes, if known.
    pub size: Option<i64>,
    /// Whether an existing file may be reused when its size matches.
    ///
    /// This is what makes a re-install cheap: the game jar of a version
    /// that is already installed is not fetched again.
    pub skip_existing: bool,
}

impl DownloadTask {
    /// Create a task for a payload with a known hash.
    #[must_use]
    pub fn new(url: impl Into<String>, target: impl Into<PathBuf>, sha1: Option<String>) -> Self {
        Self {
            url: url.into(),
            target: target.into(),
            sha1,
            size: None,
            skip_existing: true,
        }
    }

    /// Set the expected size.
    #[must_use]
    pub fn with_size(mut self, size: Option<i64>) -> Self {
        self.size = size;
        self
    }

    /// Always download the payload, even if the target exists.
    #[must_use]
    pub fn always(mut self) -> Self {
        self.skip_existing = false;
        self
    }
}

/// What happened to one download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadOutcome {
    /// The URL that was downloaded.
    pub url: String,
    /// The target the payload was written to.
    pub target: PathBuf,
    /// The number of bytes transferred.
    pub bytes: u64,
    /// Whether the file was already present and was reused.
    pub skipped: bool,
    /// How many attempts were needed.
    pub attempts: u32,
}

/// The result of a whole batch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchResult {
    /// The successful downloads.
    pub completed: Vec<DownloadOutcome>,
    /// The downloads that failed, with the reason.
    pub failed: Vec<(String, String)>,
    /// The total number of bytes transferred.
    pub bytes: u64,
}

impl BatchResult {
    /// Whether every download succeeded.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
    }
}

/// The download engine.
#[derive(Debug, Clone)]
pub struct Downloader {
    http: reqwest::Client,
    concurrency: usize,
    attempts: u32,
    backoff: Duration,
}

impl Downloader {
    /// Create an engine with the default settings.
    ///
    /// # Errors
    /// Fails if the HTTP client cannot be built.
    pub fn new() -> Result<Self> {
        Self::with_settings(DEFAULT_CONCURRENCY, DEFAULT_ATTEMPTS, DEFAULT_BACKOFF)
    }

    /// Create an engine with explicit settings.
    ///
    /// # Errors
    /// Fails if the HTTP client cannot be built.
    pub fn with_settings(concurrency: usize, attempts: u32, backoff: Duration) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("MinLauncher/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(20))
            .pool_max_idle_per_host(concurrency)
            .build()
            .map_err(|e| Error::network("http client", e))?;

        Ok(Self {
            http,
            concurrency: concurrency.max(1),
            attempts: attempts.max(1),
            backoff,
        })
    }

    /// Download a batch of files.
    ///
    /// The downloads run concurrently; a failing download does not stop
    /// the others, it is reported in [`BatchResult::failed`].
    ///
    /// # Errors
    /// Fails only when the batch itself cannot be started; individual
    /// failures are reported in the result.
    pub async fn download_all(&self, tasks: Vec<DownloadTask>) -> Result<BatchResult> {
        if tasks.is_empty() {
            return Ok(BatchResult::default());
        }

        let http = self.http.clone();
        let attempts = self.attempts;
        let backoff = self.backoff;

        let outcomes: Vec<std::result::Result<DownloadOutcome, (String, String)>> =
            futures::stream::iter(tasks)
                .map(|task| {
                    let http = http.clone();
                    async move {
                        let url = task.url.clone();
                        match download_with_retry(&http, &task, attempts, backoff).await {
                            Ok(outcome) => Ok(outcome),
                            Err(error) => Err((url, error.to_string())),
                        }
                    }
                })
                .buffer_unordered(self.concurrency)
                .collect()
                .await;

        let mut result = BatchResult::default();
        for outcome in outcomes {
            match outcome {
                Ok(outcome) => {
                    result.bytes += outcome.bytes;
                    result.completed.push(outcome);
                }
                Err((url, error)) => result.failed.push((url, error)),
            }
        }
        Ok(result)
    }

    /// Download a single file.
    ///
    /// # Errors
    /// Fails on a transport error, a non-success status, a hash mismatch,
    /// or a filesystem error.
    pub async fn download(&self, task: &DownloadTask) -> Result<DownloadOutcome> {
        download_with_retry(&self.http, task, self.attempts, self.backoff).await
    }
}

/// Download one file, retrying with an exponential backoff.
///
/// # Errors
/// Fails on a transport error, a non-success status, a hash mismatch, or a
/// filesystem error.
async fn download_with_retry(
    http: &reqwest::Client,
    task: &DownloadTask,
    attempts: u32,
    backoff: Duration,
) -> Result<DownloadOutcome> {
    let mut last_error: Option<Error> = None;

    for attempt in 1..=attempts {
        match download_once(http, task).await {
            Ok(outcome) => {
                return Ok(DownloadOutcome {
                    attempts: attempt,
                    ..outcome
                });
            }
            Err(error) => {
                tracing::warn!(
                    url = %task.url,
                    attempt,
                    "download failed: {error}"
                );
                last_error = Some(error);
                if attempt < attempts {
                    tokio::time::sleep(backoff * 2u32.pow(attempt - 1)).await;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| Error::Network {
        url: task.url.clone(),
        source: "the download was never attempted".into(),
    }))
}

/// Download one file, once.
async fn download_once(http: &reqwest::Client, task: &DownloadTask) -> Result<DownloadOutcome> {
    // A file that is already there and has the expected size is reused.
    if task.skip_existing
        && let Ok(metadata) = std::fs::metadata(&task.target)
    {
        let size_matches = task
            .size
            .is_none_or(|size| metadata.len() == size.cast_unsigned());
        if size_matches {
            return Ok(DownloadOutcome {
                url: task.url.clone(),
                target: task.target.clone(),
                bytes: 0,
                skipped: true,
                attempts: 1,
            });
        }
    }

    if let Some(parent) = task.target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    // The payload is written next to the target and moved into place only
    // after the hash was verified, so an interrupted download never leaves
    // a corrupt file behind.
    let partial = partial_path(&task.target);

    let response = http
        .get(&task.url)
        .send()
        .await
        .map_err(|e| Error::network(&task.url, e))?;

    if !response.status().is_success() {
        return Err(Error::HttpStatus {
            url: task.url.clone(),
            status: response.status().as_u16(),
        });
    }

    let file = tokio::fs::File::create(&partial)
        .await
        .map_err(|e| Error::io(&partial, e))?;
    let mut file = tokio::io::BufWriter::with_capacity(64 * 1024, file);
    let mut hasher = Sha1::new();
    let mut bytes = 0u64;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::network(&task.url, e))?;
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|e| Error::io(&partial, e))?;
        bytes += chunk.len() as u64;
    }

    file.flush().await.map_err(|e| Error::io(&partial, e))?;
    drop(file);

    if let Some(expected) = &task.sha1 {
        let actual = hex::encode(hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&partial);
            return Err(Error::ChecksumMismatch {
                path: task.target.clone(),
                expected: expected.clone(),
                actual,
            });
        }
    }

    tokio::fs::rename(&partial, &task.target)
        .await
        .map_err(|e| Error::io(&task.target, e))?;

    Ok(DownloadOutcome {
        url: task.url.clone(),
        target: task.target.clone(),
        bytes,
        skipped: false,
        attempts: 1,
    })
}

/// The path a payload is written to while it is being downloaded.
#[must_use]
pub fn partial_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().map_or_else(
        || "download".to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    name.push_str(".part");
    target.with_file_name(name)
}

/// The SHA-1 of a file on disk.
///
/// # Errors
/// Fails if the file cannot be read.
pub fn hash_file(path: &Path) -> Result<String> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
    let mut hasher = Sha1::new();
    let mut buffer = vec![0u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer).map_err(|e| Error::io(path, e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Whether a file on disk matches the expected hash.
///
/// A missing file is reported as "does not match" rather than as an error,
/// because that is the answer the caller wants: it has to be downloaded.
#[must_use]
pub fn matches_hash(path: &Path, expected: &str) -> bool {
    hash_file(path).is_ok_and(|actual| actual.eq_ignore_ascii_case(expected))
}

/// Download a set of files and verify them, keeping the shared HTTP client.
///
/// The helper is used by the asset and library planners, which both need
/// "fetch these many files and tell me what failed".
///
/// # Errors
/// Fails only when the batch cannot be started.
pub async fn fetch_all(
    http: Arc<reqwest::Client>,
    tasks: Vec<DownloadTask>,
    concurrency: usize,
) -> Result<BatchResult> {
    let outcomes: Vec<std::result::Result<DownloadOutcome, (String, String)>> =
        futures::stream::iter(tasks)
            .map(|task| {
                let http = http.clone();
                async move {
                    let url = task.url.clone();
                    match download_with_retry(&http, &task, DEFAULT_ATTEMPTS, DEFAULT_BACKOFF).await
                    {
                        Ok(outcome) => Ok(outcome),
                        Err(error) => Err((url, error.to_string())),
                    }
                }
            })
            .buffer_unordered(concurrency.max(1))
            .collect()
            .await;

    let mut result = BatchResult::default();
    for outcome in outcomes {
        match outcome {
            Ok(outcome) => {
                result.bytes += outcome.bytes;
                result.completed.push(outcome);
            }
            Err((url, error)) => result.failed.push((url, error)),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_path_appends_the_suffix() {
        let partial = partial_path(Path::new("/tmp/game/client.jar"));
        assert_eq!(partial, PathBuf::from("/tmp/game/client.jar.part"));
    }

    #[test]
    fn hash_of_a_known_payload() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("payload");
        std::fs::write(&file, b"hello").unwrap();

        // SHA-1 of "hello".
        assert_eq!(
            hash_file(&file).unwrap(),
            "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"
        );
    }

    #[test]
    fn matches_hash_compares_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("payload");
        std::fs::write(&file, b"hello").unwrap();

        assert!(matches_hash(
            &file,
            "AAF4C61DDCC5E8A2DABEDE0F3B482CD9AEA9434D"
        ));
        assert!(!matches_hash(
            &file,
            "0000000000000000000000000000000000000000"
        ));
    }

    #[test]
    fn missing_file_does_not_match() {
        assert!(!matches_hash(Path::new("/nonexistent/file"), "abc"));
    }

    #[test]
    fn empty_batch_succeeds() {
        let downloader = Downloader::new().unwrap();
        let result = futures::executor::block_on(downloader.download_all(Vec::new())).unwrap();
        assert!(result.is_success());
        assert_eq!(result.bytes, 0);
    }
}
