use serde::{Deserialize, Serialize};

/// Subscription tree node. A node is either a `feed` (has `feed_url`) or a
/// `folder` (a grouping container). `children` is populated only for the
/// tree returned by `list_feeds`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedNode {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub kind: String, // "feed" | "folder"
    pub title: String,
    pub feed_url: Option<String>,
    pub site_url: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub sort_order: i64,
    pub fetch_error: Option<String>,
    pub fetch_fail_count: i64,
    pub last_fetched_at: Option<String>,
    /// Unread count for this node. For folders this is the recursive sum of
    /// all descendant feeds, computed in Rust.
    pub unread_count: i64,
    #[serde(default)]
    pub children: Vec<FeedNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleSummary {
    pub id: i64,
    pub feed_id: i64,
    pub feed_title: String,
    pub title: String,
    pub link: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub plain_text: String,
    pub is_starred: bool,
    pub is_read: bool,
    /// Whether the reader already has a usable body without opening the source:
    /// background full text was stored, or the feed's own body is long enough to
    /// read in-app. Drives list prefetch scope and the mouse-click "open original"
    /// fallback.
    pub fulltext_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Article {
    pub id: i64,
    pub feed_id: i64,
    pub feed_title: String,
    pub title: String,
    pub link: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub content_html: String,
    /// Extracted full-article HTML when background Readability found richer
    /// content than the feed-provided `content_html`. `None` until fetched.
    pub fulltext_html: Option<String>,
    pub is_starred: bool,
    pub is_read: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticlePage {
    pub items: Vec<ArticleSummary>,
    pub total: i64,
}

/// Filter for `list_articles`. `status` is one of `all` | `unread` | `starred`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ArticleFilter {
    pub feed_id: Option<i64>,
    pub folder_id: Option<i64>,
    pub status: String,
    pub query: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

impl Default for ArticleFilter {
    fn default() -> Self {
        Self {
            feed_id: None,
            folder_id: None,
            status: "all".to_string(),
            query: None,
            limit: 100,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub refresh_interval_minutes: i64,
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            refresh_interval_minutes: 30,
            theme: "system".to_string(),
        }
    }
}

/// Partial settings update: only provided fields are written.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsInput {
    pub refresh_interval_minutes: Option<i64>,
    pub theme: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub feeds_added: i64,
    pub feeds_skipped: i64,
    pub folders_created: i64,
}

/// Intermediate representation of a feed item produced by the parser, ready
/// to be upserted into the database.
#[derive(Debug, Clone)]
pub struct NewArticle {
    pub guid: String,
    pub title: String,
    pub link: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub content_html: String,
    pub plain_text: String,
}

/// Result of probing/parsing a remote feed (used when creating a subscription
/// and when refreshing).
#[derive(Debug, Clone)]
pub struct ParsedFeed {
    pub title: Option<String>,
    pub site_url: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub articles: Vec<NewArticle>,
}
