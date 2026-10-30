pub mod content;
pub mod fetcher;
pub mod opml;
pub mod parse;

use fetcher::FetchOutcome;

use crate::db::feeds::FeedRow;
use crate::db::{articles, feeds};
use crate::error::{AppError, AppResult};
use crate::models::ParsedFeed;
use crate::state::AppState;

/// Outcome of refreshing a single subscription.
pub struct RefreshResult {
    pub feed_id: i64,
    pub new_count: i64,
    pub not_modified: bool,
}

fn join_err(e: tokio::task::JoinError) -> AppError {
    AppError::Db(format!("后台任务失败：{e}"))
}

async fn record_error(state: &AppState, feed_id: i64, msg: String) {
    let pool = state.pool.clone();
    let _ = tokio::task::spawn_blocking(move || -> AppResult<()> {
        let conn = pool.get()?;
        feeds::record_fetch_error(&conn, feed_id, &msg)
    })
    .await;
}

/// Fetches and parses a feed URL without touching the database. Used both to
/// probe a brand-new subscription and to refresh an existing one.
async fn fetch_parse(
    state: &AppState,
    url: &str,
) -> AppResult<(ParsedFeed, Option<String>, Option<String>)> {
    let outcome = fetcher::fetch(&state.client, url, None, None).await;
    match outcome? {
        FetchOutcome::NotModified => Err(AppError::Parse("服务器返回未修改，但内容不可用".into())),
        FetchOutcome::Updated {
            bytes,
            etag,
            last_modified,
        } => Ok((parse::parse_bytes(&bytes)?, etag, last_modified)),
    }
}

/// Probes `url`, validating it parses as a feed. Returns the parsed feed plus
/// the validator headers, without persisting anything.
pub async fn probe(
    state: &AppState,
    url: &str,
) -> AppResult<(ParsedFeed, Option<String>, Option<String>)> {
    fetch_parse(state, url).await
}

/// Refreshes an existing subscription: network + parse happen on the async
/// side, all SQLite writes are pushed to a blocking thread.
pub async fn refresh_feed_row(state: &AppState, feed: FeedRow) -> AppResult<RefreshResult> {
    let url = match feed.feed_url.clone() {
        Some(u) => u,
        None => return Err(AppError::Invalid("该订阅项不是可抓取的 feed".into())),
    };

    let outcome = fetcher::fetch(
        &state.client,
        &url,
        feed.etag.as_deref(),
        feed.last_modified.as_deref(),
    )
    .await;
    let (parsed, etag, last_modified) = match outcome {
        Ok(FetchOutcome::NotModified) => {
            let pool = state.pool.clone();
            let id = feed.id;
            let n = tokio::task::spawn_blocking(move || -> AppResult<i64> {
                let conn = pool.get()?;
                feeds::record_fetch_success(&conn, id)?;
                Ok(0)
            })
            .await
            .map_err(join_err)??;
            return Ok(RefreshResult {
                feed_id: id,
                new_count: n,
                not_modified: true,
            });
        }
        Ok(FetchOutcome::Updated {
            bytes,
            etag,
            last_modified,
        }) => match parse::parse_bytes(&bytes) {
            Ok(p) => (p, etag, last_modified),
            Err(e) => {
                record_error(state, feed.id, e.to_string()).await;
                return Err(e);
            }
        },
        Err(e) => {
            record_error(state, feed.id, e.to_string()).await;
            return Err(e);
        }
    };

    let pool = state.pool.clone();
    let id = feed.id;
    let new_count = tokio::task::spawn_blocking(move || -> AppResult<i64> {
        let mut conn = pool.get()?;
        let tx = conn.transaction()?;
        let n = articles::upsert_articles(&tx, id, &parsed.articles)?;
        feeds::set_conditional_headers(&tx, id, etag.as_deref(), last_modified.as_deref())?;
        feeds::record_fetch_success(&tx, id)?;
        tx.commit()?;
        Ok(n)
    })
    .await
    .map_err(join_err)??;

    Ok(RefreshResult {
        feed_id: id,
        new_count,
        not_modified: false,
    })
}

/// Creates a new subscription: probe first (no DB write on failure), then
/// persist the feed together with its first batch of articles atomically.
pub async fn create_feed(
    state: &AppState,
    url: &str,
    folder_id: Option<i64>,
) -> AppResult<(i64, i64)> {
    let normalized = normalize_url(url);
    // reject obvious duplicates before any network hit
    {
        let pool = state.pool.clone();
        let u = normalized.clone();
        let dup = tokio::task::spawn_blocking(move || -> AppResult<bool> {
            let conn = pool.get()?;
            Ok(feeds::find_feed_by_url(&conn, &u)?.is_some())
        })
        .await
        .map_err(join_err)??;
        if dup {
            return Err(AppError::Invalid("该订阅源已存在".into()));
        }
    }

    let (parsed, etag, last_modified) = probe(state, &normalized).await?;
    let total_items = parsed.articles.len() as i64;

    let pool = state.pool.clone();
    let title = parsed.title.clone().unwrap_or_else(|| normalized.clone());
    let site_url = parsed.site_url.clone();
    let description = parsed.description.clone();
    let icon_url = parsed.icon_url.clone();

    let new_id = tokio::task::spawn_blocking(move || -> AppResult<i64> {
        let mut conn = pool.get()?;
        let tx = conn.transaction()?;
        let fid = feeds::insert_feed(
            &tx,
            &title,
            &normalized,
            site_url.as_deref(),
            description.as_deref(),
            icon_url.as_deref(),
            folder_id,
        )?;
        articles::upsert_articles(&tx, fid, &parsed.articles)?;
        if etag.is_some() || last_modified.is_some() {
            feeds::set_conditional_headers(&tx, fid, etag.as_deref(), last_modified.as_deref())?;
        }
        feeds::record_fetch_success(&tx, fid)?;
        tx.commit()?;
        Ok(fid)
    })
    .await
    .map_err(join_err)??;

    Ok((new_id, total_items))
}

/// Best-effort scheme normalisation so users can paste `example.com/feed`.
fn normalize_url(raw: &str) -> String {
    let s = raw.trim();
    if s.contains("://") {
        s.to_string()
    } else {
        format!("https://{s}")
    }
}
