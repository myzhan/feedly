use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{AppError, AppResult};
use crate::models::FeedNode;

use super::now_iso;

/// Raw database row for a `feeds` record.
#[derive(Debug, Clone)]
pub struct FeedRow {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub kind: String,
    pub title: String,
    pub feed_url: Option<String>,
    pub site_url: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub sort_order: i64,
    pub fetch_error: Option<String>,
    pub fetch_fail_count: i64,
    pub last_fetched_at: Option<String>,
}

const COLS: &str = "id, parent_id, kind, title, feed_url, site_url, description, icon_url, etag, last_modified, sort_order, fetch_error, fetch_fail_count, last_fetched_at";

fn row_to_feed(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeedRow> {
    Ok(FeedRow {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        kind: row.get(2)?,
        title: row.get(3)?,
        feed_url: row.get(4)?,
        site_url: row.get(5)?,
        description: row.get(6)?,
        icon_url: row.get(7)?,
        etag: row.get(8)?,
        last_modified: row.get(9)?,
        sort_order: row.get(10)?,
        fetch_error: row.get(11)?,
        fetch_fail_count: row.get(12)?,
        last_fetched_at: row.get(13)?,
    })
}

pub fn insert_folder(conn: &Connection, title: &str, parent_id: Option<i64>) -> AppResult<i64> {
    conn.execute(
        "INSERT INTO feeds (parent_id, kind, title, sort_order, created_at) VALUES (?1, 'folder', ?2, 0, ?3)",
        params![parent_id, title, now_iso()],
    )?;
    Ok(conn.last_insert_rowid())
}

#[allow(clippy::too_many_arguments)]
pub fn insert_feed(
    conn: &Connection,
    title: &str,
    feed_url: &str,
    site_url: Option<&str>,
    description: Option<&str>,
    icon_url: Option<&str>,
    parent_id: Option<i64>,
) -> AppResult<i64> {
    conn.execute(
        "INSERT INTO feeds (parent_id, kind, title, feed_url, site_url, description, icon_url, sort_order, created_at)
         VALUES (?1, 'feed', ?2, ?3, ?4, ?5, ?6, 0, ?7)",
        params![parent_id, title, feed_url, site_url, description, icon_url, now_iso()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn find_feed_by_url(conn: &Connection, url: &str) -> AppResult<Option<FeedRow>> {
    let sql = format!("SELECT {COLS} FROM feeds WHERE feed_url = ?1 LIMIT 1");
    let found = conn.query_row(&sql, params![url], row_to_feed).optional()?;
    Ok(found)
}

pub fn get_feed(conn: &Connection, id: i64) -> AppResult<FeedRow> {
    let sql = format!("SELECT {COLS} FROM feeds WHERE id = ?1");
    conn.query_row(&sql, params![id], row_to_feed)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("订阅 {id}")))
}

pub fn list_all(conn: &Connection) -> AppResult<Vec<FeedRow>> {
    let sql = format!(
        "SELECT {COLS} FROM feeds ORDER BY kind DESC, sort_order ASC, title COLLATE NOCASE ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], row_to_feed)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Updates title, parent folder and ordering. `parent_id` of `None` moves the
/// node to the root. Refuses to make a folder its own descendant.
pub fn update_feed(
    conn: &Connection,
    id: i64,
    title: &str,
    parent_id: Option<i64>,
    sort_order: i64,
) -> AppResult<()> {
    if let Some(pid) = parent_id {
        if pid == id {
            return Err(AppError::Invalid("不能把订阅移动到自身".into()));
        }
        if descendant_feed_ids(conn, id)?.contains(&pid) {
            return Err(AppError::Invalid("不能移动到自身的子级".into()));
        }
    }
    let n = conn.execute(
        "UPDATE feeds SET title = ?2, parent_id = ?3, sort_order = ?4 WHERE id = ?1",
        params![id, title, parent_id, sort_order],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("订阅 {id}")));
    }
    Ok(())
}

pub fn delete_feed(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM feeds WHERE id = ?1", params![id])?;
    Ok(())
}

fn node_from(row: FeedRow, unread: i64) -> FeedNode {
    FeedNode {
        id: row.id,
        parent_id: row.parent_id,
        kind: row.kind,
        title: row.title,
        feed_url: row.feed_url,
        site_url: row.site_url,
        description: row.description,
        icon_url: row.icon_url,
        sort_order: row.sort_order,
        fetch_error: row.fetch_error,
        fetch_fail_count: row.fetch_fail_count,
        last_fetched_at: row.last_fetched_at,
        unread_count: unread,
        children: Vec::new(),
    }
}

/// Returns a single node (with its own unread count for feeds, 0 for folders).
/// Used right after create/update so the frontend can refresh one row cheaply.
pub fn get_feed_node(conn: &Connection, id: i64) -> AppResult<FeedNode> {
    let row = get_feed(conn, id)?;
    let unread = if row.kind == "feed" {
        conn.query_row(
            "SELECT COUNT(*) FROM articles WHERE feed_id = ?1 AND read_at IS NULL",
            params![id],
            |r| r.get::<_, i64>(0),
        )?
    } else {
        0
    };
    Ok(node_from(row, unread))
}

pub fn set_conditional_headers(
    conn: &Connection,
    id: i64,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> AppResult<()> {
    conn.execute(
        "UPDATE feeds SET etag = ?2, last_modified = ?3 WHERE id = ?1",
        params![id, etag, last_modified],
    )?;
    Ok(())
}

/// Marks a successful fetch: records time, clears any stored error.
pub fn record_fetch_success(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE feeds SET last_fetched_at = ?2, fetch_error = NULL, fetch_fail_count = 0 WHERE id = ?1",
        params![id, now_iso()],
    )?;
    Ok(())
}

pub fn record_fetch_error(conn: &Connection, id: i64, err: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE feeds SET fetch_error = ?2, fetch_fail_count = fetch_fail_count + 1 WHERE id = ?1",
        params![id, err],
    )?;
    Ok(())
}

/// All recursive feed (leaf) ids under `folder_id`, including `folder_id`
/// itself when it is a feed. Returns an empty vec for an empty folder.
pub fn descendant_feed_ids(conn: &Connection, node_id: i64) -> AppResult<Vec<i64>> {
    let sql = "WITH RECURSIVE subtree(id, parent_id, kind) AS (
                 SELECT id, parent_id, kind FROM feeds WHERE id = ?1
                 UNION ALL
                 SELECT f.id, f.parent_id, f.kind FROM feeds f JOIN subtree s ON f.parent_id = s.id
               )
               SELECT id FROM subtree WHERE kind = 'feed'";
    let mut stmt = conn.prepare_cached(sql)?;
    let ids = stmt
        .query_map(params![node_id], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

/// Unread (read_at IS NULL) article count per feed id.
pub fn unread_counts(conn: &Connection) -> AppResult<HashMap<i64, i64>> {
    let mut stmt = conn
        .prepare("SELECT feed_id, COUNT(*) FROM articles WHERE read_at IS NULL GROUP BY feed_id")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
    let mut map = HashMap::new();
    for row in rows {
        let (fid, cnt) = row?;
        map.insert(fid, cnt);
    }
    Ok(map)
}

/// Builds the full subscription tree with per-node and rolled-up unread counts.
pub fn build_tree(conn: &Connection) -> AppResult<Vec<FeedNode>> {
    let feeds = list_all(conn)?;
    let unread = unread_counts(conn)?;

    // node_id -> children rows (preserving the ORDER BY from list_all)
    let mut children_map: HashMap<Option<i64>, Vec<FeedRow>> = HashMap::new();
    for f in &feeds {
        children_map.entry(f.parent_id).or_default().push(f.clone());
    }

    fn build(
        row: &FeedRow,
        children_map: &HashMap<Option<i64>, Vec<FeedRow>>,
        unread: &HashMap<i64, i64>,
    ) -> FeedNode {
        let kids = children_map.get(&Some(row.id)).cloned().unwrap_or_default();
        let child_nodes: Vec<FeedNode> = kids
            .iter()
            .map(|k| build(k, children_map, unread))
            .collect();
        // roll up descendant unread into folder totals
        let own = if row.kind == "feed" {
            *unread.get(&row.id).unwrap_or(&0)
        } else {
            0
        };
        let rolled: i64 = own + child_nodes.iter().map(|c| c.unread_count).sum::<i64>();
        FeedNode {
            id: row.id,
            parent_id: row.parent_id,
            kind: row.kind.clone(),
            title: row.title.clone(),
            feed_url: row.feed_url.clone(),
            site_url: row.site_url.clone(),
            description: row.description.clone(),
            icon_url: row.icon_url.clone(),
            sort_order: row.sort_order,
            fetch_error: row.fetch_error.clone(),
            fetch_fail_count: row.fetch_fail_count,
            last_fetched_at: row.last_fetched_at.clone(),
            unread_count: rolled,
            children: child_nodes,
        }
    }

    let roots = children_map.get(&None).cloned().unwrap_or_default();
    Ok(roots
        .iter()
        .map(|r| build(r, &children_map, &unread))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_pool;

    #[test]
    fn tree_rolls_up_unread_and_detects_cycle() {
        let (pool, _d) = test_pool();
        let conn = pool.get().unwrap();
        let folder = insert_folder(&conn, "News", None).unwrap();
        let a = insert_feed(
            &conn,
            "A",
            "https://a.example/feed",
            None,
            None,
            None,
            Some(folder),
        )
        .unwrap();
        let b = insert_feed(
            &conn,
            "B",
            "https://b.example/feed",
            None,
            None,
            None,
            Some(folder),
        )
        .unwrap();
        let root_feed =
            insert_feed(&conn, "C", "https://c.example/feed", None, None, None, None).unwrap();

        // seed unread articles
        for fid in [a, b, root_feed] {
            conn.execute(
                "INSERT INTO articles (feed_id, guid, title, content_html, plain_text, created_at) VALUES (?1, ?2, 't', 'c', 'p', '2026-01-01')",
                params![fid, format!("g-{fid}")],
            )
            .unwrap();
        }

        let tree = build_tree(&conn).unwrap();
        assert_eq!(tree.len(), 2); // folder + root feed
        let folder_node = tree.iter().find(|n| n.id == folder).unwrap();
        assert_eq!(folder_node.unread_count, 2);
        assert_eq!(folder_node.children.len(), 2);

        // moving a folder into its own child must fail
        assert!(update_feed(&conn, folder, "News", Some(a), 0).is_err());
    }

    #[test]
    fn duplicate_feed_url_rejected() {
        let (pool, _d) = test_pool();
        let conn = pool.get().unwrap();
        insert_feed(&conn, "A", "https://dup/feed", None, None, None, None).unwrap();
        assert!(insert_feed(&conn, "B", "https://dup/feed", None, None, None, None).is_err());
    }
}
