use std::hash::{Hash, Hasher};

use feed_rs::model::{Entry, Feed, Link, Text};
use feed_rs::parser;

use crate::error::{AppError, AppResult};
use crate::models::{NewArticle, ParsedFeed};

use super::content::{fallback_html, plain_text, sanitize_html};

const SUMMARY_LIMIT: usize = 400;

fn text_content(t: &Option<Text>) -> Option<String> {
    t.as_ref()
        .map(|x| x.content.clone())
        .filter(|s| !s.trim().is_empty())
}

fn first_link(links: &[Link]) -> Option<String> {
    links
        .iter()
        .map(|l| l.href.trim())
        .find(|h| !h.is_empty())
        .map(|h| h.to_string())
}

/// Chooses the human-facing site URL, preferring a non-`self` link (Atom
/// exposes the feed URL itself with `rel="self"`).
fn site_url(feed: &Feed) -> Option<String> {
    feed.links
        .iter()
        .find(|l| l.rel.as_deref() != Some("self") && !l.href.trim().is_empty())
        .map(|l| l.href.clone())
        .or_else(|| first_link(&feed.links))
}

fn entry_author(e: &Entry) -> Option<String> {
    e.authors
        .iter()
        .map(|p| p.name.clone().unwrap_or_default())
        .find(|n| !n.trim().is_empty())
}

fn entry_date(e: &Entry) -> Option<String> {
    e.published.or(e.updated).map(|d| d.to_rfc3339())
}

/// Stable id for dedup: prefer feed-rs id (source guid or a deterministic
/// link hash), then the link, then a content hash of title+date+link.
fn entry_guid(e: &Entry) -> String {
    let id = e.id.trim();
    if !id.is_empty() {
        return e.id.clone();
    }
    if let Some(l) = first_link(&e.links) {
        return l;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text_content(&e.title).hash(&mut h);
    entry_date(e).hash(&mut h);
    first_link(&e.links).hash(&mut h);
    format!("h{:016x}", h.finish())
}

fn entry_content_html(e: &Entry, title: &str, link: Option<&str>) -> String {
    // Prefer full content, fall back to summary, then to a title/link stub.
    let raw = e
        .content
        .as_ref()
        .and_then(|c| c.body.clone())
        .filter(|b| !b.trim().is_empty())
        .or_else(|| text_content(&e.summary));
    match raw {
        Some(body) => sanitize_html(&body),
        None => fallback_html(title, link),
    }
}

fn convert_entry(e: &Entry) -> NewArticle {
    let title = text_content(&e.title).unwrap_or_default();
    let link = first_link(&e.links);
    let content_html = entry_content_html(e, &title, link.as_deref());
    NewArticle {
        guid: entry_guid(e),
        title,
        link,
        author: entry_author(e),
        published_at: entry_date(e),
        plain_text: plain_text(&content_html, SUMMARY_LIMIT),
        content_html,
    }
}

pub fn convert_feed(feed: Feed) -> ParsedFeed {
    let icon_url = feed
        .icon
        .as_ref()
        .map(|i| i.uri.clone())
        .or_else(|| feed.logo.as_ref().map(|i| i.uri.clone()));
    ParsedFeed {
        title: text_content(&feed.title),
        site_url: site_url(&feed),
        description: text_content(&feed.description),
        icon_url,
        articles: feed.entries.iter().map(convert_entry).collect(),
    }
}

/// Parses raw feed bytes (RSS 1.0/2.0, Atom or JSON Feed) into our model.
pub fn parse_bytes(bytes: &[u8]) -> AppResult<ParsedFeed> {
    let feed = parser::parse(bytes).map_err(|e| AppError::Parse(e.to_string()))?;
    Ok(convert_feed(feed))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS2: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
  <title>Example feed</title>
  <link>https://example.com/</link>
  <description>Desc</description>
  <item>
    <title>First post</title>
    <link>https://example.com/first</link>
    <guid isPermaLink="false">abc-123</guid>
    <pubDate>Sun, 06 Sep 2009 16:20:00 +0000</pubDate>
    <description>&lt;p&gt;Hello &lt;b&gt;world&lt;/b&gt;&lt;/p&gt;&lt;script&gt;bad()&lt;/script&gt;</description>
    <content:encoded xmlns:content="http://purl.org/rss/1.0/modules/content/">&lt;p&gt;Full body&lt;/p&gt;</content:encoded>
  </item>
</channel></rss>"#;

    const ATOM: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Atom example</title>
  <link href="https://atom.example/self" rel="self"/>
  <link href="https://atom.example/site"/>
  <updated>2020-01-01T00:00:00Z</updated>
  <id>urn:uuid:feed</id>
  <entry>
    <title>Atom entry</title>
    <link href="https://atom.example/e1"/>
    <id>tag:atom,2020:e1</id>
    <updated>2020-01-02T00:00:00Z</updated>
    <summary>Summary text</summary>
  </entry>
</feed>"#;

    #[test]
    fn rss2_maps_fields_and_prefers_content_encoded() {
        let f = parse_bytes(RSS2.as_bytes()).unwrap();
        assert_eq!(f.title.as_deref(), Some("Example feed"));
        assert_eq!(f.site_url.as_deref(), Some("https://example.com/"));
        assert_eq!(f.articles.len(), 1);
        let a = &f.articles[0];
        assert_eq!(a.guid, "abc-123");
        assert_eq!(a.title, "First post");
        assert_eq!(a.link.as_deref(), Some("https://example.com/first"));
        assert!(a.content_html.contains("Full body"));
        assert!(!a.content_html.contains("script"));
        assert!(a.published_at.is_some());
    }

    #[test]
    fn atom_picks_non_self_site_url_and_summary_body() {
        let f = parse_bytes(ATOM.as_bytes()).unwrap();
        assert_eq!(f.site_url.as_deref(), Some("https://atom.example/site"));
        let a = &f.articles[0];
        assert_eq!(a.guid, "tag:atom,2020:e1");
        assert!(a.content_html.contains("Summary text"));
    }

    #[test]
    fn guid_is_stable_across_parses() {
        let f1 = parse_bytes(RSS2.as_bytes()).unwrap();
        let f2 = parse_bytes(RSS2.as_bytes()).unwrap();
        assert_eq!(f1.articles[0].guid, f2.articles[0].guid);
    }

    #[test]
    fn missing_content_falls_back_to_title_link() {
        let xml = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>t</title><link>l</link>
        <item><title>No body</title><link>https://e/nb</link><guid>g</guid></item></channel></rss>"#;
        let f = parse_bytes(xml.as_bytes()).unwrap();
        assert!(f.articles[0].content_html.contains("No body"));
    }
}
