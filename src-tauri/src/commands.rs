use rusqlite::Connection;
use tauri::{AppHandle, Manager, State};

use crate::db::{articles, feeds, settings};
use crate::error::{AppError, AppResult};
use crate::feed;
use crate::image_cache::ImageRef;
use crate::models::{
    Article, ArticleFilter, ArticlePage, FeedNode, ImportReport, Settings as SettingsModel,
    SettingsInput,
};
use crate::scheduler;
use crate::state::AppState;

/// Runs a synchronous SQLite closure on a blocking thread, checking out a
/// pooled connection. Keeps every command off the async event loop.
async fn blocking<T, F>(state: &AppState, f: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Connection) -> AppResult<T> + Send + 'static,
{
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || {
        let conn = pool.get()?;
        f(&conn)
    })
    .await
    .map_err(|e| AppError::Db(format!("后台任务失败：{e}")))?
}

/// Resolves the set of feed ids a filter/folder targets. `None` = all feeds.
fn resolve_scope(
    conn: &Connection,
    feed_id: Option<i64>,
    folder_id: Option<i64>,
) -> AppResult<Option<Vec<i64>>> {
    if let Some(fid) = feed_id {
        Ok(Some(vec![fid]))
    } else if let Some(folder) = folder_id {
        Ok(Some(feeds::descendant_feed_ids(conn, folder)?))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub async fn list_feeds(state: State<'_, AppState>) -> Result<Vec<FeedNode>, String> {
    blocking(&state, feeds::build_tree)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn create_folder(
    state: State<'_, AppState>,
    title: String,
    parent_id: Option<i64>,
) -> Result<FeedNode, String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("文件夹名称不能为空".into());
    }
    let node = blocking(&state, move |conn| {
        let id = feeds::insert_folder(conn, &title, parent_id)?;
        feeds::get_feed_node(conn, id)
    })
    .await
    .map_err::<String, _>(Into::into)?;
    Ok(node)
}

#[tauri::command]
pub async fn create_feed(
    state: State<'_, AppState>,
    url: String,
    folder_id: Option<i64>,
) -> Result<FeedNode, String> {
    let (id, _new_count) = feed::create_feed(&state, &url, folder_id)
        .await
        .map_err::<String, _>(Into::into)?;
    blocking(&state, move |conn| feeds::get_feed_node(conn, id))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn update_feed(
    state: State<'_, AppState>,
    id: i64,
    title: String,
    folder_id: Option<i64>,
    sort_order: i64,
) -> Result<FeedNode, String> {
    let title = title.trim().to_string();
    let node = blocking(&state, move |conn| {
        feeds::update_feed(conn, id, &title, folder_id, sort_order)?;
        feeds::get_feed_node(conn, id)
    })
    .await
    .map_err::<String, _>(Into::into)?;
    Ok(node)
}

#[tauri::command]
pub async fn delete_feed(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    blocking(&state, move |conn| feeds::delete_feed(conn, id))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn list_articles(
    state: State<'_, AppState>,
    filter: ArticleFilter,
) -> Result<ArticlePage, String> {
    let page = blocking(&state, move |conn| {
        let ids = resolve_scope(conn, filter.feed_id, filter.folder_id)?;
        let (items, total) = articles::list_articles(conn, &filter, ids.as_deref())?;
        Ok(ArticlePage { items, total })
    })
    .await
    .map_err::<String, _>(Into::into)?;
    Ok(page)
}

#[tauri::command]
pub async fn get_article(state: State<'_, AppState>, id: i64) -> Result<Article, String> {
    blocking(&state, move |conn| articles::get_article(conn, id))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn mark_read(state: State<'_, AppState>, ids: Vec<i64>) -> Result<(), String> {
    blocking(&state, move |conn| articles::mark_read(conn, &ids))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn mark_unread(state: State<'_, AppState>, ids: Vec<i64>) -> Result<(), String> {
    blocking(&state, move |conn| articles::mark_unread(conn, &ids))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn set_starred(
    state: State<'_, AppState>,
    id: i64,
    starred: bool,
) -> Result<bool, String> {
    blocking(&state, move |conn| articles::set_starred(conn, id, starred))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn mark_all_read(
    state: State<'_, AppState>,
    feed_id: Option<i64>,
    folder_id: Option<i64>,
) -> Result<i64, String> {
    blocking(&state, move |conn| {
        let ids = resolve_scope(conn, feed_id, folder_id)?;
        articles::mark_all_read(conn, ids.as_deref())
    })
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn refresh_feed(app: AppHandle, id: i64) -> Result<i64, String> {
    let res = scheduler::refresh_one(app, id)
        .await
        .map_err::<String, _>(Into::into)?;
    Ok(res.new_count)
}

#[tauri::command]
pub async fn refresh_all(app: AppHandle) -> Result<(), String> {
    // Fire-and-forget: progress is delivered through events.
    tauri::async_runtime::spawn(scheduler::refresh_all(app));
    Ok(())
}

#[tauri::command]
pub async fn export_opml(state: State<'_, AppState>) -> Result<String, String> {
    blocking(&state, feed::opml::export_opml)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn import_opml(state: State<'_, AppState>, xml: String) -> Result<ImportReport, String> {
    blocking(&state, move |conn| feed::opml::import_opml(conn, &xml))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<SettingsModel, String> {
    blocking(&state, settings::get_settings)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    input: SettingsInput,
) -> Result<SettingsModel, String> {
    blocking(&state, move |conn| settings::update_settings(conn, &input))
        .await
        .map_err(Into::into)
}

/// Queues background full-text extraction for a batch of article ids (the
/// currently loaded list). Returns immediately; each id is de-duplicated and
/// throttled inside `fulltext::request`, and results arrive via the
/// `fulltext-ready` event.
#[tauri::command]
pub async fn prefetch_fulltext(app: AppHandle, ids: Vec<i64>) -> Result<(), String> {
    for id in ids {
        crate::fulltext::request(app.clone(), id);
    }
    Ok(())
}

/// Ensures the given (absolute) image URLs are cached locally, returning the
/// subset that succeeded together with their local file paths.
#[tauri::command]
pub async fn cache_images(
    state: State<'_, AppState>,
    urls: Vec<String>,
) -> Result<Vec<ImageRef>, String> {
    Ok(crate::image_cache::cache(&state, urls).await)
}

/// Label of the child webview that renders an article's original page in place.
const ORIGINAL_VIEW_LABEL: &str = "original-view";

/// Clips the child webview to rounded corners so the embedded page reads as a
/// card inset in the pane instead of a raw rectangular window. HTML-level
/// rounding cannot clip a native view, hence the platform layer.
#[cfg(target_os = "macos")]
fn round_webview_corners(view: &tauri::Webview) {
    /// Corner radius (logical px), matching the card's `rounded-xl` in the UI.
    const RADIUS: f64 = 12.0;

    let _ = view.with_webview(|wv| {
        use objc2::msg_send;
        use objc2::runtime::{AnyObject, Bool};
        let ptr = wv.inner();
        if ptr.is_null() {
            return;
        }
        unsafe {
            let obj: &AnyObject = &*(ptr as *const AnyObject);
            let _: () = msg_send![obj, setWantsLayer: Bool::YES];
            let layer: *mut AnyObject = msg_send![obj, layer];
            if !layer.is_null() {
                let layer: &AnyObject = &*layer;
                let _: () = msg_send![layer, setCornerRadius: RADIUS];
                let _: () = msg_send![layer, setMasksToBounds: Bool::YES];
            }
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn round_webview_corners(_view: &tauri::Webview) {}

fn close_original_view_impl(app: &AppHandle) {
    if let Some(view) = app.get_webview(ORIGINAL_VIEW_LABEL) {
        let _ = view.close();
    }
}

/// Covers the article content area with a native child webview showing `url`,
/// so the original page renders in-app instead of opening the system browser.
/// Any existing view is replaced so the page loads fresh at the top. `x/y/width/
/// height` are the target area in logical (CSS) pixels within the main window.
#[tauri::command]
pub async fn open_original_view(
    app: AppHandle,
    url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let parsed = url
        .parse::<tauri::Url>()
        .map_err(|e| format!("无效的原文地址：{e}"))?;
    close_original_view_impl(&app);
    let window = app
        .get_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?;
    let builder = tauri::webview::WebviewBuilder::new(
        ORIGINAL_VIEW_LABEL,
        tauri::WebviewUrl::External(parsed),
    );
    let view = window
        .add_child(
            builder,
            tauri::LogicalPosition::new(x, y),
            tauri::LogicalSize::new(width, height),
        )
        .map_err(|e| e.to_string())?;
    round_webview_corners(&view);
    Ok(())
}

/// Repositions/resizes the original-page view, e.g. when the window or panes
/// change size. No-op when no view is open.
#[tauri::command]
pub async fn set_original_view_bounds(
    app: AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    if let Some(view) = app.get_webview(ORIGINAL_VIEW_LABEL) {
        view.set_position(tauri::LogicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
        view.set_size(tauri::LogicalSize::new(width, height))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Closes the original-page view, restoring the extracted/feed body.
#[tauri::command]
pub async fn close_original_view(app: AppHandle) -> Result<(), String> {
    close_original_view_impl(&app);
    Ok(())
}
