use rusqlite::types::Value as SqlValue;
use rusqlite::{params, Connection, OptionalExtension, ToSql};

use crate::error::{AppError, AppResult};
use crate::feed::content;
use crate::models::{Article, ArticleFilter, ArticleSummary, NewArticle};

use super::now_iso;

const MAX_LIMIT: i64 = 500;

/// Minimum uncapped visible-text length (`body_chars`) for a feed-provided body
/// to count as already readable in-app, so the list can flag rows that need no
/// full-text prefetch (`fulltext_ready`).
const READABLE_MIN_CHARS: i64 = 500;

/// Inserts new articles for a feed, ignoring already-known guids. Returns the
/// number of rows actually inserted (used to publish `article-added`).
pub fn upsert_articles(conn: &Connection, feed_id: i64, items: &[NewArticle]) -> AppResult<i64> {
    let created = now_iso();
    let mut inserted = 0i64;
    for a in items {
        // Visible-text length of the full body, uncapped (plain_text is only a
        // truncated list preview), used to decide `fulltext_ready`.
        let body_chars = content::plain_text(&a.content_html, usize::MAX)
            .chars()
            .count() as i64;
        let n = conn.execute(
            "INSERT OR IGNORE INTO articles
               (feed_id, guid, title, link, author, published_at, content_html, plain_text, body_chars, is_starred, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10)",
            params![
                feed_id,
                a.guid,
                a.title,
                a.link,
                a.author,
                a.published_at,
                a.content_html,
                a.plain_text,
                body_chars,
                created
            ],
        )?;
        inserted += n as i64;
    }
    Ok(inserted)
}

/// Escapes LIKE wildcards so a user query is matched literally.
fn escape_like(q: &str) -> String {
    let mut out = String::with_capacity(q.len());
    for c in q.chars() {
        if c == '%' || c == '_' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn build_conditions(filter: &ArticleFilter, feed_ids: Option<&[i64]>) -> (String, Vec<SqlValue>) {
    let mut clauses: Vec<String> = Vec::new();
    let mut args: Vec<SqlValue> = Vec::new();

    if let Some(ids) = feed_ids {
        if ids.is_empty() {
            clauses.push("0".to_string()); // no feeds in scope -> match nothing
        } else {
            let ph = vec!["?"; ids.len()].join(",");
            clauses.push(format!("a.feed_id IN ({ph})"));
            args.extend(ids.iter().map(|i| SqlValue::Integer(*i)));
        }
    }

    match filter.status.as_str() {
        "unread" => clauses.push("a.read_at IS NULL".to_string()),
        "starred" => clauses.push("a.is_starred = 1".to_string()),
        _ => {}
    }

    if let Some(q) = filter
        .query
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let pat = format!("%{}%", escape_like(q));
        clauses.push("(a.title LIKE ? ESCAPE '\\' OR a.plain_text LIKE ? ESCAPE '\\')".to_string());
        args.push(SqlValue::Text(pat.clone()));
        args.push(SqlValue::Text(pat));
    }

    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };
    (where_sql, args)
}

/// Returns one page of article summaries plus the total count matching the
/// filter. `feed_ids` is the resolved scope (from feed_id or folder_id);
/// `None` means "all feeds".
pub fn list_articles(
    conn: &Connection,
    filter: &ArticleFilter,
    feed_ids: Option<&[i64]>,
) -> AppResult<(Vec<ArticleSummary>, i64)> {
    let (where_sql, base_args) = build_conditions(filter, feed_ids);

    let count_sql = format!("SELECT COUNT(*) FROM articles a {where_sql}");
    let arg_refs: Vec<&dyn ToSql> = base_args.iter().map(|v| v as &dyn ToSql).collect();
    let total: i64 = conn.query_row(&count_sql, arg_refs.as_slice(), |r| r.get(0))?;

    let limit = filter.limit.clamp(1, MAX_LIMIT);
    let offset = filter.offset.max(0);

    let items_sql = format!(
        "SELECT a.id, a.feed_id, f.title, a.title, a.link, a.author, a.published_at,
                a.plain_text, a.is_starred, (a.read_at IS NOT NULL),
                (a.fulltext_html IS NOT NULL
                   OR a.body_chars >= {READABLE_MIN_CHARS})
         FROM articles a JOIN feeds f ON f.id = a.feed_id
         {where_sql}
         ORDER BY (a.published_at IS NULL), a.published_at DESC, a.id DESC
         LIMIT ? OFFSET ?"
    );
    let mut item_args = base_args.clone();
    item_args.push(SqlValue::Integer(limit));
    item_args.push(SqlValue::Integer(offset));
    let item_refs: Vec<&dyn ToSql> = item_args.iter().map(|v| v as &dyn ToSql).collect();

    let mut stmt = conn.prepare(&items_sql)?;
    let rows = stmt.query_map(item_refs.as_slice(), |r| {
        Ok(ArticleSummary {
            id: r.get(0)?,
            feed_id: r.get(1)?,
            feed_title: r.get(2)?,
            title: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
            link: r.get(4)?,
            author: r.get(5)?,
            published_at: r.get(6)?,
            plain_text: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
            is_starred: r.get::<_, i64>(8)? != 0,
            is_read: r.get::<_, i64>(9)? != 0,
            fulltext_ready: r.get::<_, i64>(10)? != 0,
        })
    })?;
    let items = rows.collect::<Result<Vec<_>, _>>()?;
    Ok((items, total))
}

pub fn get_article(conn: &Connection, id: i64) -> AppResult<Article> {
    let sql = "SELECT a.id, a.feed_id, f.title, a.title, a.link, a.author, a.published_at,
                      a.content_html, a.fulltext_html, a.is_starred, (a.read_at IS NOT NULL)
               FROM articles a JOIN feeds f ON f.id = a.feed_id WHERE a.id = ?1";
    conn.query_row(sql, params![id], |r| {
        Ok(Article {
            id: r.get(0)?,
            feed_id: r.get(1)?,
            feed_title: r.get(2)?,
            title: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
            link: r.get(4)?,
            author: r.get(5)?,
            published_at: r.get(6)?,
            content_html: r.get(7)?,
            fulltext_html: r.get(8)?,
            is_starred: r.get::<_, i64>(9)? != 0,
            is_read: r.get::<_, i64>(10)? != 0,
        })
    })
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("文章 {id}")))
}

/// Persists the outcome of a full-text extraction attempt. `html` is the
/// cleaned article body, or `None` when nothing better than the feed content
/// was found. Always stamps `fulltext_at` so the article is not retried.
pub fn set_fulltext(conn: &Connection, id: i64, html: Option<&str>) -> AppResult<()> {
    conn.execute(
        "UPDATE articles SET fulltext_html = ?2, fulltext_at = ?3 WHERE id = ?1",
        params![id, html, now_iso()],
    )?;
    Ok(())
}

/// Whether a background full-text fetch is worth running for `id`. Returns
/// `None` when there is nothing to do (article missing, no link, already
/// attempted, or already has full text). Otherwise the article's `link` and
/// its current `content_html`, so the caller can compare the extracted body's
/// visible-text length against the feed-provided one.
pub fn fulltext_pending(conn: &Connection, id: i64) -> AppResult<Option<(String, String)>> {
    let row: Option<(Option<String>, String)> = conn
        .query_row(
            "SELECT link, content_html FROM articles
             WHERE id = ?1 AND fulltext_at IS NULL AND fulltext_html IS NULL",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (link, content_html) = match row {
        Some(v) => v,
        None => return Ok(None),
    };
    match link {
        Some(l) if !l.trim().is_empty() => Ok(Some((l, content_html))),
        _ => Ok(None),
    }
}

/// Ids of unread articles in `feed_id` that are good full-text prefetch
/// candidates (have a link, not yet fetched or attempted), newest first.
pub fn unread_fulltext_candidates(
    conn: &Connection,
    feed_id: i64,
    limit: i64,
) -> AppResult<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM articles
         WHERE feed_id = ?1 AND read_at IS NULL
           AND fulltext_html IS NULL AND fulltext_at IS NULL
           AND link IS NOT NULL
         ORDER BY (published_at IS NULL), published_at DESC, id DESC
         LIMIT ?2",
    )?;
    let ids = stmt
        .query_map(params![feed_id, limit], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

fn set_read_for_ids(conn: &Connection, ids: &[i64], read: bool) -> AppResult<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let ph = vec!["?"; ids.len()].join(",");
    let sql = if read {
        format!("UPDATE articles SET read_at = ?1 WHERE id IN ({ph}) AND read_at IS NULL")
    } else {
        format!("UPDATE articles SET read_at = NULL WHERE id IN ({ph})")
    };
    let mut args: Vec<SqlValue> = Vec::with_capacity(ids.len() + 1);
    if read {
        args.push(SqlValue::Text(now_iso()));
    }
    args.extend(ids.iter().map(|i| SqlValue::Integer(*i)));
    let refs: Vec<&dyn ToSql> = args.iter().map(|v| v as &dyn ToSql).collect();
    conn.execute(&sql, refs.as_slice())?;
    Ok(())
}

pub fn mark_read(conn: &Connection, ids: &[i64]) -> AppResult<()> {
    set_read_for_ids(conn, ids, true)
}

pub fn mark_unread(conn: &Connection, ids: &[i64]) -> AppResult<()> {
    set_read_for_ids(conn, ids, false)
}

pub fn set_starred(conn: &Connection, id: i64, starred: bool) -> AppResult<bool> {
    let n = conn.execute(
        "UPDATE articles SET is_starred = ?2 WHERE id = ?1",
        params![id, starred as i64],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("文章 {id}")));
    }
    Ok(starred)
}

/// Marks every unread article in the given scope as read. `feed_ids` of `None`
/// means "all feeds". Returns the number of rows changed.
pub fn mark_all_read(conn: &Connection, feed_ids: Option<&[i64]>) -> AppResult<i64> {
    let now = now_iso();
    match feed_ids {
        None => Ok(conn.execute(
            "UPDATE articles SET read_at = ?1 WHERE read_at IS NULL",
            params![now],
        )? as i64),
        Some([]) => Ok(0),
        Some(ids) => {
            let ph = vec!["?"; ids.len()].join(",");
            let sql = format!(
                "UPDATE articles SET read_at = ?1 WHERE feed_id IN ({ph}) AND read_at IS NULL"
            );
            let mut args: Vec<SqlValue> = vec![SqlValue::Text(now)];
            args.extend(ids.iter().map(|i| SqlValue::Integer(*i)));
            let refs: Vec<&dyn ToSql> = args.iter().map(|v| v as &dyn ToSql).collect();
            Ok(conn.execute(&sql, refs.as_slice())? as i64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{feeds, tests::test_pool};
    use rusqlite::params;

    fn sample(guid: &str, title: &str, body: &str) -> NewArticle {
        NewArticle {
            guid: guid.into(),
            title: title.into(),
            link: Some(format!("https://x/{guid}")),
            author: None,
            published_at: Some("2026-01-02T00:00:00+00:00".into()),
            content_html: body.into(),
            plain_text: body.into(),
        }
    }

    #[test]
    fn dedup_by_guid_counts_only_new_rows() {
        let (pool, _d) = test_pool();
        let conn = pool.get().unwrap();
        let fid = feeds::insert_feed(&conn, "F", "https://f/feed", None, None, None, None).unwrap();
        let batch = vec![sample("g1", "One", "hello"), sample("g2", "Two", "world")];
        assert_eq!(upsert_articles(&conn, fid, &batch).unwrap(), 2);
        // re-inserting the same guids inserts nothing
        assert_eq!(upsert_articles(&conn, fid, &batch).unwrap(), 0);
        // a new guid plus an existing one -> 1
        let batch2 = vec![sample("g2", "Two", "world"), sample("g3", "Three", "again")];
        assert_eq!(upsert_articles(&conn, fid, &batch2).unwrap(), 1);
    }

    #[test]
    fn deleted_article_reinserted_counts_as_new() {
        let (pool, _d) = test_pool();
        let conn = pool.get().unwrap();
        let fid = feeds::insert_feed(&conn, "F", "https://f/feed", None, None, None, None).unwrap();
        upsert_articles(&conn, fid, &[sample("g1", "One", "hi")]).unwrap();
        conn.execute("DELETE FROM articles WHERE guid='g1'", params![])
            .unwrap();
        assert_eq!(
            upsert_articles(&conn, fid, &[sample("g1", "One", "hi")]).unwrap(),
            1
        );
    }

    #[test]
    fn filter_status_and_search() {
        let (pool, _d) = test_pool();
        let conn = pool.get().unwrap();
        let fid = feeds::insert_feed(&conn, "F", "https://f/feed", None, None, None, None).unwrap();
        upsert_articles(
            &conn,
            fid,
            &[
                sample("g1", "Rust release", "1.0 body"),
                sample("g2", "Go release", "notes"),
            ],
        )
        .unwrap();
        // mark one read
        conn.execute(
            "UPDATE articles SET read_at='2026-01-03' WHERE guid='g1'",
            params![],
        )
        .unwrap();

        let f_all = ArticleFilter::default();
        let (items, total) = list_articles(&conn, &f_all, Some(&[fid])).unwrap();
        assert_eq!(total, 2);
        assert_eq!(items.len(), 2);

        let f_unread = ArticleFilter {
            status: "unread".into(),
            ..Default::default()
        };
        let (_, total) = list_articles(&conn, &f_unread, Some(&[fid])).unwrap();
        assert_eq!(total, 1);

        let f_q = ArticleFilter {
            query: Some("Rust".into()),
            ..Default::default()
        };
        let (items, total) = list_articles(&conn, &f_q, Some(&[fid])).unwrap();
        assert_eq!(total, 1);
        assert_eq!(items[0].title, "Rust release");

        // wildcard in query must be treated literally
        let f_pct = ArticleFilter {
            query: Some("%".into()),
            ..Default::default()
        };
        let (_, total) = list_articles(&conn, &f_pct, Some(&[fid])).unwrap();
        assert_eq!(total, 0);
    }

    #[test]
    fn mark_all_read_respects_scope() {
        let (pool, _d) = test_pool();
        let conn = pool.get().unwrap();
        let f1 = feeds::insert_feed(&conn, "A", "https://a/feed", None, None, None, None).unwrap();
        let f2 = feeds::insert_feed(&conn, "B", "https://b/feed", None, None, None, None).unwrap();
        upsert_articles(&conn, f1, &[sample("g1", "1", "b")]).unwrap();
        upsert_articles(&conn, f2, &[sample("g2", "2", "b")]).unwrap();
        // scope only to f1
        let changed = mark_all_read(&conn, Some(&[f1])).unwrap();
        assert_eq!(changed, 1);
        let still_unread: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM articles WHERE read_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(still_unread, 1);
    }

    #[test]
    fn fulltext_ready_uses_uncapped_body_length() {
        let (pool, _d) = test_pool();
        let conn = pool.get().unwrap();
        let fid = feeds::insert_feed(&conn, "F", "https://f/feed", None, None, None, None).unwrap();
        let long_body = "<p>".to_string() + &"word ".repeat(600) + "</p>";
        upsert_articles(
            &conn,
            fid,
            &[
                sample("short", "Short", "tiny summary"),
                sample("long", "Long", &long_body),
            ],
        )
        .unwrap();
        let (items, _) = list_articles(&conn, &ArticleFilter::default(), Some(&[fid])).unwrap();
        let ready = |t: &str| items.iter().find(|a| a.title == t).unwrap().fulltext_ready;
        // A full feed body is readable in-app even without extraction; a short
        // summary is not. Uses body_chars, not the truncated plain_text.
        assert!(ready("Long"), "long body should be ready");
        assert!(!ready("Short"), "short summary should not be ready");
    }
}
