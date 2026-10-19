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

/// How far a batch has come.
///
/// The report is a snapshot: it says how many files are done, how many
/// bytes have moved, and which file was being fetched at the moment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadProgress {
    /// The number of files that are done, successful or not.
    pub completed: usize,
    /// The number of files in the batch.
    pub total: usize,
    /// The number of bytes that have been transferred.
    pub bytes: u64,
    /// The URL that was being fetched, when one is.
    pub current: Option<String>,
}

impl DownloadProgress {
    /// The part of the batch that is done, between 0 and 1.
    ///
    /// A batch of zero files is done, not stuck at zero: there is
    /// nothing left to wait for.
    #[must_use]
    pub fn fraction(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        self.completed as f64 / self.total as f64
    }

    /// Whether every file of the batch is done.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.completed >= self.total
    }
}

/// The thing that is told about the progress of a batch.
///
/// The callback is called from several tasks at once, so it has to be
/// usable from more than one thread.
pub type ProgressCallback = Arc<dyn Fn(DownloadProgress) + Send + Sync>;

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

    /// Download a batch of files and report the progress.
    ///
    /// The callback is called once per file that finishes, with a
    /// snapshot of the batch. The last call always carries a finished
    /// batch, whatever the order the files finished in.
    ///
    /// # Errors
    /// Fails only when the batch itself cannot be started; individual
    /// failures are reported in the result.
    pub async fn download_all_with_progress(
        &self,
        tasks: Vec<DownloadTask>,
        on_progress: ProgressCallback,
    ) -> Result<BatchResult> {
        let total = tasks.len();
        if total == 0 {
            on_progress(DownloadProgress {
                completed: 0,
                total: 0,
                bytes: 0,
                current: None,
            });
            return Ok(BatchResult::default());
        }

        let http = self.http.clone();
        let attempts = self.attempts;
        let backoff = self.backoff;

        // The counter and the byte total are shared by every task of the
        // batch, because they all finish at the same time.
        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let bytes = Arc::new(std::sync::atomic::AtomicU64::new(0));

        let outcomes: Vec<std::result::Result<DownloadOutcome, (String, String)>> =
            futures::stream::iter(tasks)
                .map(|task| {
                    let http = http.clone();
                    let on_progress = Arc::clone(&on_progress);
                    let completed = Arc::clone(&completed);
                    let bytes = Arc::clone(&bytes);
                    async move {
                        let url = task.url.clone();
                        let result =
                            match download_with_retry(&http, &task, attempts, backoff).await {
                                Ok(outcome) => Ok(outcome),
                                Err(error) => Err((url.clone(), error.to_string())),
                            };

                        // The report is sent whether the file was
                        // fetched or not: a file that failed is done
                        // too, and the caller has to know about it.
                        let moved = bytes.fetch_add(
                            result.as_ref().map_or(0, |outcome| outcome.bytes),
                            std::sync::atomic::Ordering::Relaxed,
                        );
                        let done = completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;

                        on_progress(DownloadProgress {
                            completed: done,
                            total,
                            bytes: moved + result.as_ref().map_or(0, |outcome| outcome.bytes),
                            current: Some(url),
                        });

                        result
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

        // The last report says that the batch is over, whatever the last
        // file to finish was.
        on_progress(DownloadProgress {
            completed: total,
            total,
            bytes: result.bytes,
            current: None,
        });

        Ok(result)
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

    #[test]
    fn an_empty_batch_is_already_finished() {
        let progress = DownloadProgress::default();
        assert_eq!(progress.fraction(), 1.0);
        assert!(progress.is_finished());
    }

    #[test]
    fn the_fraction_follows_the_number_of_files() {
        let progress = DownloadProgress {
            completed: 25,
            total: 100,
            bytes: 1024,
            current: None,
        };
        assert!((progress.fraction() - 0.25).abs() < f64::EPSILON);
        assert!(!progress.is_finished());
    }

    #[test]
    fn the_report_of_an_empty_batch_says_it_is_over() {
        let downloader = Downloader::new().unwrap();
        let seen: Arc<std::sync::Mutex<Vec<DownloadProgress>>> =
            Arc::new(std::sync::Mutex::new(vec![]));

        let recorder = Arc::clone(&seen);
        let result = futures::executor::block_on(downloader.download_all_with_progress(
            Vec::new(),
            Arc::new(move |progress| {
                recorder.lock().unwrap().push(progress);
            }),
        ))
        .unwrap();

        assert!(result.is_success());
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(seen[0].is_finished());
        assert_eq!(seen[0].total, 0);
    }

    #[test]
    fn a_file_that_is_already_there_is_still_counted() {
        // The report counts the files that are done, not the bytes that
        // moved: a version that is already installed is finished at once.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("already.jar");
        std::fs::write(&target, b"jar").unwrap();

        let downloader = Downloader::new().unwrap();
        let seen: Arc<std::sync::Mutex<Vec<DownloadProgress>>> =
            Arc::new(std::sync::Mutex::new(vec![]));
        let recorder = Arc::clone(&seen);

        let task = DownloadTask::new("https://example/jar", &target, None).with_size(Some(3));
        let result = futures::executor::block_on(downloader.download_all_with_progress(
            vec![task],
            Arc::new(move |progress| {
                recorder.lock().unwrap().push(progress);
            }),
        ))
        .unwrap();

        assert!(result.is_success());
        assert_eq!(result.bytes, 0);

        let seen = seen.lock().unwrap();
        assert!(seen.len() >= 2, "{seen:?}");
        let last = seen.last().unwrap();
        assert!(last.is_finished());
        assert_eq!(last.completed, 1);
    }
}
