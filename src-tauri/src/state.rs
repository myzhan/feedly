use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::Client;
use tokio::sync::Semaphore;

use crate::db::DbPool;
use crate::error::AppResult;

/// Shared application state managed by Tauri. Cheap to clone (pool, clients and
/// semaphores are internally reference-counted).
#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    /// Client used for feed fetches (identifies as the app).
    pub client: Client,
    /// Client used for full-text and image fetches. Uses a mainstream browser
    /// UA so article pages and hotlinked images are less likely to be blocked.
    pub web_client: Client,
    /// Re-entrancy guard so scheduled and manual full refreshes never overlap.
    pub refreshing: Arc<AtomicBool>,
    /// Directory where downloaded article images are cached for offline/speed.
    pub image_cache_dir: PathBuf,
    /// Bounds concurrent background full-text fetches (network politeness).
    pub fulltext_sem: Arc<Semaphore>,
    /// Bounds concurrent image downloads within a single cache batch.
    pub image_sem: Arc<Semaphore>,
    /// Article ids with a full-text fetch currently in flight (dedup).
    pub inflight: Arc<Mutex<HashSet<i64>>>,
}

/// A mainstream desktop UA, used only for on-demand full-text/image fetches.
/// Must be a *complete* modern browser UA: a truncated one (e.g. `Chrome/124.0`
/// with no build segment) is flagged as a bot and Cloudflare-fronted origins
/// answer HTTP 520, even though a real browser opens the same page instantly.
const WEB_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

impl AppState {
    pub fn new(pool: DbPool, data_dir: PathBuf) -> AppResult<Self> {
        let client = Client::builder()
            .user_agent(concat!("feedly/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(5))
            .gzip(true)
            .brotli(true)
            .deflate(true)
            .build()
            .map_err(|e| crate::error::AppError::Network(e.to_string()))?;

        let web_client = Client::builder()
            .user_agent(WEB_USER_AGENT)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::limited(10))
            .gzip(true)
            .brotli(true)
            .deflate(true)
            .build()
            .map_err(|e| crate::error::AppError::Network(e.to_string()))?;

        let image_cache_dir = data_dir.join("images");
        std::fs::create_dir_all(&image_cache_dir)
            .map_err(|e| crate::error::AppError::Io(e.to_string()))?;

        Ok(Self {
            pool,
            client,
            web_client,
            refreshing: Arc::new(AtomicBool::new(false)),
            image_cache_dir,
            fulltext_sem: Arc::new(Semaphore::new(6)),
            image_sem: Arc::new(Semaphore::new(4)),
            inflight: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    /// Attempts to acquire the refresh lock. Returns `true` if the caller now
    /// owns it and must call `release_refresh` when finished.
    pub fn try_begin_refresh(&self) -> bool {
        !self.refreshing.swap(true, Ordering::SeqCst)
    }

    pub fn release_refresh(&self) {
        self.refreshing.store(false, Ordering::SeqCst);
    }

    /// Marks `id` as in-flight if not already. Returns `false` when another
    /// fetch is already running for it, so the caller should skip.
    pub fn begin_fulltext(&self, id: i64) -> bool {
        self.inflight
            .lock()
            .map(|mut s| s.insert(id))
            .unwrap_or(true)
    }

    pub fn end_fulltext(&self, id: i64) {
        if let Ok(mut s) = self.inflight.lock() {
            s.remove(&id);
        }
    }
}
