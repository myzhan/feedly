use std::time::Duration;

use futures::stream::{self, StreamExt};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::db::feeds::FeedRow;
use crate::db::{articles, feeds, settings};
use crate::error::AppResult;
use crate::feed;
use crate::fulltext;
use crate::state::AppState;

pub const EV_STARTED: &str = "refresh-started";
pub const EV_PROGRESS: &str = "refresh-progress";
pub const EV_DONE: &str = "refresh-done";
pub const EV_ARTICLE_ADDED: &str = "article-added";

/// Max feeds fetched concurrently during a full refresh.
const CONCURRENCY: usize = 8;
/// Max unread articles per feed to prefetch full text for after a refresh.
const PREFETCH_CAP: i64 = 20;

#[derive(Clone, Serialize)]
pub struct Started {
    pub total: usize,
}

#[derive(Clone, Serialize)]
pub struct Progress {
    pub feed_id: i64,
    pub new_count: i64,
    pub not_modified: bool,
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct Done {
    pub processed: usize,
    pub total: usize,
    pub new_articles: i64,
    pub skipped: bool,
}

fn owned_state(app: &AppHandle) -> AppState {
    let s = app.state::<AppState>();
    (*s).clone()
}

/// Queues background full-text fetches for the newest unread articles of a
/// feed. Fire-and-forget: each `fulltext::request` is itself throttled by the
/// shared semaphore, so enqueueing a bounded batch is cheap.
pub fn prefetch_fulltext(app: AppHandle, feed_id: i64) {
    let state = owned_state(&app);
    tauri::async_runtime::spawn(async move {
        let pool = state.pool.clone();
        let ids = tokio::task::spawn_blocking(move || -> AppResult<Vec<i64>> {
            let conn = pool.get()?;
            articles::unread_fulltext_candidates(&conn, feed_id, PREFETCH_CAP)
        })
        .await;
        if let Ok(Ok(ids)) = ids {
            for id in ids {
                fulltext::request(app.clone(), id);
            }
        }
    });
}

async fn snapshot_feeds(state: &AppState) -> Vec<FeedRow> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> AppResult<Vec<FeedRow>> {
        let conn = pool.get()?;
        feeds::list_all(&conn)
    })
    .await
    .ok()
    .and_then(|r| r.ok())
    .unwrap_or_default()
    .into_iter()
    .filter(|f| f.kind == "feed")
    .collect()
}

/// Refreshes every subscription with bounded concurrency, guarded against
/// re-entrant runs. Emits progress events so the UI can show a progress bar.
pub async fn refresh_all(app: AppHandle) {
    let state = owned_state(&app);
    if !state.try_begin_refresh() {
        // A refresh is already running; report a no-op completion.
        let _ = app.emit(
            EV_DONE,
            Done {
                processed: 0,
                total: 0,
                new_articles: 0,
                skipped: true,
            },
        );
        return;
    }

    let feeds = snapshot_feeds(&state).await;
    let total = feeds.len();
    let _ = app.emit(EV_STARTED, Started { total });

    let mut stream = stream::iter(feeds)
        .map(|f| {
            let st = state.clone();
            let id = f.id;
            async move {
                let res = feed::refresh_feed_row(&st, f).await;
                (id, res)
            }
        })
        .buffer_unordered(CONCURRENCY);

    let mut processed = 0usize;
    let mut new_articles = 0i64;
    let mut refreshed_with_new: Vec<i64> = Vec::new();
    while let Some((id, res)) = stream.next().await {
        processed += 1;
        let payload = match res {
            Ok(r) => {
                new_articles += r.new_count;
                if r.new_count > 0 {
                    refreshed_with_new.push(r.feed_id);
                    let _ = app.emit(EV_ARTICLE_ADDED, serde_json::json!({ "feedId": r.feed_id }));
                }
                Progress {
                    feed_id: r.feed_id,
                    new_count: r.new_count,
                    not_modified: r.not_modified,
                    error: None,
                }
            }
            Err(e) => Progress {
                feed_id: id,
                new_count: 0,
                not_modified: false,
                error: Some(e.to_string()),
            },
        };
        let _ = app.emit(EV_PROGRESS, &payload);
    }

    state.release_refresh();
    // Warm the full-text cache for feeds that brought in new articles.
    for fid in refreshed_with_new {
        prefetch_fulltext(app.clone(), fid);
    }
    let _ = app.emit(
        EV_DONE,
        Done {
            processed,
            total,
            new_articles,
            skipped: false,
        },
    );
}

async fn current_interval(state: &AppState) -> i64 {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> AppResult<i64> {
        let conn = pool.get()?;
        Ok(settings::get_settings(&conn)?.refresh_interval_minutes)
    })
    .await
    .ok()
    .and_then(|r| r.ok())
    .unwrap_or(0)
}

/// Long-running scheduled refresh loop. Reads the interval from settings every
/// cycle so changes apply without a restart. `0` disables automatic refresh.
/// A one-shot fetch runs 5s after startup (when enabled).
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let mut first = true;
        loop {
            let interval = {
                let state = owned_state(&app);
                current_interval(&state).await
            };
            if interval <= 0 {
                // auto-refresh disabled: poll settings periodically
                first = false;
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }
            if !first {
                let secs = (interval.max(1) as u64) * 60;
                tokio::time::sleep(Duration::from_secs(secs)).await;
            }
            refresh_all(app.clone()).await;
            first = false;
        }
    });
}

/// Performs a single feed's refresh and emits its progress event.
pub async fn refresh_one(app: AppHandle, feed_id: i64) -> AppResult<crate::feed::RefreshResult> {
    let state = owned_state(&app);
    let feed_row = {
        let pool = state.pool.clone();
        tokio::task::spawn_blocking(move || -> AppResult<FeedRow> {
            let conn = pool.get()?;
            feeds::get_feed(&conn, feed_id)
        })
        .await
        .map_err(|e| crate::error::AppError::Db(e.to_string()))??
    };
    let res = feed::refresh_feed_row(&state, feed_row).await?;
    let _ = app.emit(
        EV_PROGRESS,
        Progress {
            feed_id: res.feed_id,
            new_count: res.new_count,
            not_modified: res.not_modified,
            error: None,
        },
    );
    if res.new_count > 0 {
        let _ = app.emit(
            EV_ARTICLE_ADDED,
            serde_json::json!({ "feedId": res.feed_id }),
        );
        prefetch_fulltext(app.clone(), res.feed_id);
    }
    Ok(res)
}
