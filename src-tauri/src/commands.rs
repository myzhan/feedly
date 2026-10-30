use rusqlite::Connection;
use tauri::{AppHandle, State};

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
