//! Background full-article extraction (Readability).
//!
//! The loaded article list is prefetched (see `commands::prefetch_fulltext`) and
//! new articles are warmed after a refresh (see `scheduler::prefetch_fulltext`).
//! Each request runs a non-blocking task that downloads the linked page,
//! extracts the main article body, and — only if it is meaningfully richer than
//! the feed-provided content — persists it to `articles.fulltext_html`. The
//! renderer prefers the stored full text; a `fulltext-ready` event lets the UI
//! refetch the article and its list row in place.

use tauri::{AppHandle, Emitter, Manager};

use crate::db::articles;
use crate::error::AppResult;
use crate::feed::{content, fetcher};
use crate::state::AppState;

/// Event emitted to the webview once a better full-text body is available.
pub const EV_FULLTEXT_READY: &str = "fulltext-ready";

/// Minimum visible-text length for an extraction to be worth showing.
const MIN_EXTRACT_CHARS: usize = 500;
/// How much longer the extracted body must be than the feed content to replace
/// it, so we never downgrade pages where the feed already carried full text.
const REPLACE_RATIO: f64 = 1.3;

/// Kicks off a background extraction for `article_id`, returning immediately.
pub fn request(app: AppHandle, article_id: i64) {
    let state = app.state::<AppState>().inner().clone();
    tauri::async_runtime::spawn(async move {
        // Acquire a permit before claiming the id, so tasks still queued for a
        // permit don't hold the in-flight mark.
        let Ok(_permit) = state.fulltext_sem.acquire().await else {
            return;
        };
        if !state.begin_fulltext(article_id) {
            return;
        }
        run(&app, &state, article_id).await;
        state.end_fulltext(article_id);
    });
}

async fn run(app: &AppHandle, state: &AppState, article_id: i64) {
    // Is there anything to do? (Skips already-tried / no-link articles.)
    let pool = state.pool.clone();
    let pending = tokio::task::spawn_blocking(move || -> AppResult<Option<(String, String)>> {
        let conn = pool.get()?;
        articles::fulltext_pending(&conn, article_id)
    })
    .await;

    let (link, current_html) = match pending {
        Ok(Ok(Some(v))) => v,
        // Not pending (already tried / no link) or a DB/join failure: nothing
        // to do. Network errors are handled by `extract` below and leave the
        // row unstamped so a later session can retry.
        _ => return,
    };

    match extract(state, &link, &current_html).await {
        Ok(Some(html)) => {
            let pool = state.pool.clone();
            let stored = tokio::task::spawn_blocking(move || -> Result<(), String> {
                let conn = pool.get().map_err(|e| e.to_string())?;
                articles::set_fulltext(&conn, article_id, Some(&html)).map_err(|e| e.to_string())
            })
            .await;
            if matches!(stored, Ok(Ok(()))) {
                let _ = app.emit(
                    EV_FULLTEXT_READY,
                    serde_json::json!({ "articleId": article_id }),
                );
            }
        }
        // Fetched fine but nothing better: stamp as attempted so we don't retry.
        Ok(None) => {
            let pool = state.pool.clone();
            let _ = tokio::task::spawn_blocking(move || -> Result<(), String> {
                let conn = pool.get().map_err(|e| e.to_string())?;
                articles::set_fulltext(&conn, article_id, None).map_err(|e| e.to_string())
            })
            .await;
        }
        // Network/parse failure: leave un-stamped to allow a later retry.
        Err(_) => {}
    }
}

/// Downloads `link` and returns cleaned full-text HTML when it is richer than
/// `current_html`. `Ok(None)` = no better content; `Err` = transient failure.
async fn extract(state: &AppState, link: &str, current_html: &str) -> AppResult<Option<String>> {
    let outcome = fetcher::fetch(&state.web_client, link, None, None).await?;
    let bytes = match outcome {
        fetcher::FetchOutcome::NotModified => return Ok(None),
        fetcher::FetchOutcome::Updated { bytes, .. } => bytes,
    };
    let html = String::from_utf8_lossy(&bytes);
    Ok(extract_article(&html, link, current_html))
}

/// Pure (offline-testable) extraction: run Readability, sanitize, and keep the
/// result only when its visible text is substantially longer than the feed's.
pub fn extract_article(page_html: &str, url: &str, current_html: &str) -> Option<String> {
    let readability = readabilityrs::Readability::new(page_html, Some(url), None).ok()?;
    let article = readability.parse()?;
    let raw = article.content?;

    let cleaned = content::sanitize_html(&raw);
    if cleaned.trim().is_empty() {
        return None;
    }

    let new_len = article
        .text_content
        .map(|t| t.chars().count())
        .unwrap_or_else(|| content::plain_text(&cleaned, usize::MAX).chars().count());
    let cur_len = content::plain_text(current_html, usize::MAX)
        .chars()
        .count();

    let substantially_longer = new_len >= MIN_EXTRACT_CHARS
        && (cur_len == 0 || new_len as f64 > cur_len as f64 * REPLACE_RATIO);
    if substantially_longer {
        Some(cleaned)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_article_and_rejects_short_page() {
        let body = "<nav>menu</nav><article><h1>Title</h1><p>".to_string()
            + &"This is a long enough paragraph to satisfy the threshold. ".repeat(30)
            + "</p></article><footer>ads</footer>";
        let current = "<p>tl;dr</p>"; // short summary from the feed
        let out = extract_article(&body, "https://example.com/post", current);
        assert!(out.is_some(), "should extract a richer body");
        let out = out.unwrap();
        assert!(out.contains("long enough paragraph"));
        assert!(!out.contains("menu"));
        // Sanitizer must strip scripts even if present in the page.
        assert!(!out.contains("script"));
    }

    #[test]
    fn does_not_replace_content_that_is_already_full() {
        let full = "<article>".to_string()
            + &"Existing full article body that is already long enough here. ".repeat(30)
            + "</article>";
        // Feed already provides the full text of essentially the same page.
        let out = extract_article(&full, "https://example.com/post", &full);
        assert!(out.is_none(), "should not downgrade near-equal content");
    }
}
