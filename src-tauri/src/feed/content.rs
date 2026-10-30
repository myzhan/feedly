use std::sync::OnceLock;

use regex::Regex;

/// Sanitizes untrusted feed HTML using ammonia's default (already XSS-safe)
/// tag allow-list, extended with the `class` attribute for styling. The result
/// is safe to inject into the webview.
pub fn sanitize_html(html: &str) -> String {
    ammonia::Builder::default()
        .add_generic_attributes(&["class"])
        .clean(html)
        .to_string()
}

fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<[^>]+>").unwrap())
}

fn ws_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+").unwrap())
}

/// Converts (already sanitized) HTML into a short plain-text snippet used for
/// list summaries and search.
pub fn plain_text(html: &str, limit: usize) -> String {
    let no_tags = tag_re().replace_all(html, " ");
    let unescaped = decode_entities(&no_tags);
    let collapsed = ws_re().replace_all(&unescaped, " ").trim().to_string();
    truncate_chars(&collapsed, limit)
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

pub fn truncate_chars(s: &str, limit: usize) -> String {
    if s.chars().count() <= limit {
        return s.to_string();
    }
    let cut: String = s.chars().take(limit).collect();
    format!("{cut}…")
}

/// Wraps a fallback body when a feed provides no usable content, producing
/// minimal safe HTML (escaped title, plus a link to the original when known).
pub fn fallback_html(title: &str, link: Option<&str>) -> String {
    let mut out = format!("<p>{}</p>", html_escape(title));
    if let Some(l) = link {
        out.push_str(&format!(
            "<p><a href=\"{}\">{}</a></p>",
            html_escape(l),
            html_escape(title)
        ));
    }
    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_scripts_and_event_handlers() {
        let dirty =
            "<p onclick=\"evil()\">hi <script>steal()</script><img src=x onerror=bad()></p>";
        let clean = sanitize_html(dirty);
        assert!(clean.contains("hi"));
        assert!(!clean.contains("script"));
        assert!(!clean.contains("onclick"));
        assert!(!clean.contains("onerror"));
    }

    #[test]
    fn keeps_class_and_links() {
        let clean = sanitize_html("<a class=\"x\" href=\"https://e.com\">t</a>");
        assert!(clean.contains("href="));
        assert!(clean.contains("class="));
    }

    #[test]
    fn plain_text_collapses_and_truncates() {
        let p = plain_text("<p>Hello   <b>world</b></p>", 400);
        assert_eq!(p, "Hello world");
        let long = plain_text(&"<p>a</p>".repeat(1000), 10);
        assert_eq!(long.chars().count(), 11); // 10 chars + ellipsis
    }

    #[test]
    fn decodes_common_entities() {
        assert_eq!(plain_text("<p>a&nbsp;&amp;&nbsp;b</p>", 100), "a & b");
    }
}
