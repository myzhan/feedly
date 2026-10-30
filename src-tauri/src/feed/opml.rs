use rusqlite::Connection;

use crate::db::feeds::{self, FeedRow};
use crate::error::{AppError, AppResult};
use crate::models::ImportReport;

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Serializes the whole subscription tree to an OPML 2.0 document.
pub fn export_opml(conn: &Connection) -> AppResult<String> {
    let feeds = feeds::list_all(conn)?;
    let mut children: std::collections::HashMap<Option<i64>, Vec<FeedRow>> =
        std::collections::HashMap::new();
    for f in &feeds {
        children.entry(f.parent_id).or_default().push(f.clone());
    }

    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(
        "<opml version=\"2.0\">\n  <head><title>Feedly subscriptions</title></head>\n  <body>\n",
    );

    fn write_node(
        row: &FeedRow,
        children: &std::collections::HashMap<Option<i64>, Vec<FeedRow>>,
        out: &mut String,
        depth: usize,
    ) {
        let pad = "    ".repeat(depth);
        let title = xml_escape(&row.title);
        if row.kind == "folder" {
            out.push_str(&format!(
                "{pad}<outline text=\"{title}\" title=\"{title}\">\n"
            ));
            if let Some(kids) = children.get(&Some(row.id)) {
                for k in kids {
                    write_node(k, children, out, depth + 1);
                }
            }
            out.push_str(&format!("{pad}</outline>\n"));
        } else {
            let url = xml_escape(row.feed_url.as_deref().unwrap_or(""));
            let html = row
                .site_url
                .as_ref()
                .map(|s| format!(" htmlUrl=\"{}\"", xml_escape(s)))
                .unwrap_or_default();
            out.push_str(&format!(
                "{pad}<outline type=\"rss\" text=\"{title}\" title=\"{title}\" xmlUrl=\"{url}\"{html}/>\n"
            ));
        }
    }

    for root in children.get(&None).cloned().unwrap_or_default() {
        write_node(&root, &children, &mut out, 2);
    }
    out.push_str("  </body>\n</opml>\n");
    Ok(out)
}

fn get_or_create_folder(
    conn: &Connection,
    title: &str,
    parent: Option<i64>,
) -> AppResult<(i64, bool)> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM feeds WHERE kind='folder' AND title=?1 AND parent_id IS ?2 LIMIT 1",
            rusqlite::params![title, parent],
            |r| r.get(0),
        )
        .optional_ok()?;
    if let Some(id) = existing {
        return Ok((id, false));
    }
    let id = feeds::insert_folder(conn, title, parent)?;
    Ok((id, true))
}

// Small helper so we can use `?` with rusqlite Option results without importing
// OptionalExtension everywhere.
trait OptionalOk {
    fn optional_ok(self) -> AppResult<Option<i64>>;
}
impl OptionalOk for rusqlite::Result<i64> {
    fn optional_ok(self) -> AppResult<Option<i64>> {
        use rusqlite::OptionalExtension;
        Ok(self.optional()?)
    }
}

fn import_siblings(
    conn: &Connection,
    node: roxmltree::Node,
    parent: Option<i64>,
    report: &mut ImportReport,
) -> AppResult<()> {
    for outline in node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "outline")
    {
        let title = outline
            .attribute("title")
            .or_else(|| outline.attribute("text"))
            .unwrap_or("Untitled")
            .trim()
            .to_string();
        let title = if title.is_empty() {
            "Untitled".to_string()
        } else {
            title
        };

        let xml_url = outline
            .attribute("xmlUrl")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());

        if let Some(url) = xml_url {
            if feeds::find_feed_by_url(conn, url)?.is_some() {
                report.feeds_skipped += 1;
                continue;
            }
            let html = outline
                .attribute("htmlUrl")
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            feeds::insert_feed(conn, &title, url, html, None, None, parent)?;
            report.feeds_added += 1;
        } else {
            // Treat as a folder only when it actually nests outlines.
            let has_children = outline
                .children()
                .any(|c| c.is_element() && c.tag_name().name() == "outline");
            if has_children {
                let (fid, created) = get_or_create_folder(conn, &title, parent)?;
                if created {
                    report.folders_created += 1;
                }
                import_siblings(conn, outline, Some(fid), report)?;
            } else {
                // A feed outline missing xmlUrl cannot be subscribed.
                report.feeds_skipped += 1;
            }
        }
    }
    Ok(())
}

/// Imports an OPML document, creating folders and feeds. Existing feeds (by
/// url) and folders (by title+parent) are reused rather than duplicated.
pub fn import_opml(conn: &Connection, xml: &str) -> AppResult<ImportReport> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| AppError::Parse(format!("OPML 解析失败：{e}")))?;
    let root = doc.root_element();
    let body = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "body")
        .unwrap_or(root);

    let mut report = ImportReport {
        feeds_added: 0,
        feeds_skipped: 0,
        folders_created: 0,
    };
    import_siblings(conn, body, None, &mut report)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{feeds, tests::test_pool};

    #[test]
    fn export_then_import_roundtrip_into_fresh_db() {
        let (src, _d1) = test_pool();
        let conn = src.get().unwrap();
        let folder = feeds::insert_folder(&conn, "News", None).unwrap();
        feeds::insert_feed(
            &conn,
            "A",
            "https://a/feed",
            Some("https://a"),
            None,
            None,
            Some(folder),
        )
        .unwrap();
        feeds::insert_feed(&conn, "B", "https://b/feed", None, None, None, Some(folder)).unwrap();
        feeds::insert_feed(&conn, "Root", "https://root/feed", None, None, None, None).unwrap();

        let xml = export_opml(&conn).unwrap();
        assert!(xml.contains("xmlUrl=\"https://a/feed\""));

        let (dst, _d2) = test_pool();
        let dconn = dst.get().unwrap();
        let report = import_opml(&dconn, &xml).unwrap();
        assert_eq!(report.feeds_added, 3);
        assert_eq!(report.folders_created, 1);

        // Exporting the destination should contain the same set of feed urls.
        let xml2 = export_opml(&dconn).unwrap();
        for url in ["https://a/feed", "https://b/feed", "https://root/feed"] {
            assert!(xml2.contains(url), "missing {url}");
        }
        assert!(xml2.contains("News"));

        // Re-importing the same document into dst adds nothing new.
        let report2 = import_opml(&dconn, &xml).unwrap();
        assert_eq!(report2.feeds_added, 0);
        assert_eq!(report2.feeds_skipped, 3);
    }

    #[test]
    fn escapes_special_characters() {
        let (pool, _d) = test_pool();
        let conn = pool.get().unwrap();
        feeds::insert_feed(
            &conn,
            "A & B <x>",
            "https://x/feed?a=1&b=2",
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let xml = export_opml(&conn).unwrap();
        assert!(xml.contains("A &amp; B &lt;x&gt;"));
        assert!(xml.contains("a=1&amp;b=2"));
        // and it still re-parses
        let report = import_opml(&conn, &xml).unwrap();
        assert_eq!(report.feeds_skipped, 1); // already present, escaped-decoded back
    }
}
