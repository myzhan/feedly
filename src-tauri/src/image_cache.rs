//! Local cache for article images.
//!
//! On article open the frontend collects the (already absolute) image URLs and
//! calls the `cache_images` command. We download each missing image into
//! `$APPDATA/images/<sha256(url)>.<ext>`, then hand back the local paths so the
//! renderer can rewrite `<img src>` to the asset protocol. Repeat opens are
//! instant and work offline, and hotlink-protected images load because we fetch
//! them server-side (no browser Referer).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use futures::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::state::AppState;

/// Hard cap on a single cached image.
const MAX_IMAGE_BYTES: usize = 15 * 1024 * 1024;
/// Total cache size budget before LRU eviction kicks in.
const MAX_CACHE_BYTES: u64 = 500 * 1024 * 1024;

/// One cached image: the original URL and its absolute local file path.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageRef {
    pub url: String,
    pub path: String,
}

/// Ensures every `urls` is present in the cache, downloading missing ones with
/// bounded concurrency. Returns the successfully cached refs (already-cached
/// files count as immediate hits). Failures are simply omitted so the renderer
/// falls back to the remote URL.
pub async fn cache(state: &AppState, urls: Vec<String>) -> Vec<ImageRef> {
    let dir = &state.image_cache_dir;
    let existing = scan_existing(dir);

    // Dedup by url, resolving instant cache hits up front.
    let mut seen: HashSet<String> = HashSet::new();
    let mut results: Vec<ImageRef> = Vec::new();
    let mut missing: Vec<(String, String)> = Vec::new(); // (url, hex)
    for url in urls {
        if !(url.starts_with("http://") || url.starts_with("https://")) || !seen.insert(url.clone())
        {
            continue;
        }
        let hex = url_hex(&url);
        if let Some(p) = existing.get(&hex) {
            results.push(ImageRef {
                url,
                path: p.to_string_lossy().into_owned(),
            });
        } else {
            missing.push((url, hex));
        }
    }

    if missing.is_empty() {
        return results;
    }

    let sem = state.image_sem.clone();
    let client = state.web_client.clone();
    let dir = dir.clone();
    let handles: Vec<_> = missing
        .into_iter()
        .map(|(url, hex)| {
            let sem = sem.clone();
            let client = client.clone();
            let dir = dir.clone();
            tokio::spawn(async move {
                let _permit = sem.acquire().await.ok()?;
                download(&client, &dir, &url, &hex).await
            })
        })
        .collect();

    for h in handles {
        if let Ok(Some(r)) = h.await {
            results.push(r);
        }
    }

    // Opportunistically keep the cache within budget after a download batch.
    let _ = evict_if_needed(&dir).await;
    results
}

fn url_hex(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Maps cached files by their 64-char hex prefix, ignoring the extension.
fn scan_existing(dir: &Path) -> HashMap<String, PathBuf> {
    let mut map = HashMap::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let stem = name.split('.').next().unwrap_or("").to_string();
            if stem.len() == 64 {
                map.insert(stem, entry.path());
            }
        }
    }
    map
}

async fn download(client: &reqwest::Client, dir: &Path, url: &str, hex: &str) -> Option<ImageRef> {
    let resp = client.get(url).send().await.ok()?.error_for_status().ok()?;

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let bytes = read_capped(resp).await.ok()?;

    let ext = extension_for(&content_type, url);
    if ext.is_empty() {
        return None; // not a recognised image; don't store junk
    }

    let file = dir.join(format!("{hex}.{ext}"));
    let tmp = dir.join(format!("{hex}.{ext}.tmp"));
    std::fs::write(&tmp, &bytes).ok()?;
    std::fs::rename(&tmp, &file).ok()?;

    Some(ImageRef {
        url: url.to_string(),
        path: file.to_string_lossy().into_owned(),
    })
}

/// Streams the response body up to `MAX_IMAGE_BYTES`, erroring if exceeded.
async fn read_capped(resp: reqwest::Response) -> crate::error::AppResult<Vec<u8>> {
    use crate::error::AppError;
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if buf.len() + chunk.len() > MAX_IMAGE_BYTES {
            return Err(AppError::Network("图片超过 15MB 上限，已中止".into()));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// Derives a file extension from the content type, falling back to the URL's
/// own extension. Returns "" when neither indicates an image.
fn extension_for(content_type: &str, url: &str) -> String {
    let ct = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let from_ct = ct.strip_prefix("image/").unwrap_or("");
    let from_ct = match from_ct {
        "jpeg" | "jpg" => "jpg",
        "svg+xml" => "svg",
        "x-ms-bmp" => "bmp",
        other => other,
    };
    if !from_ct.is_empty() && !from_ct.contains('/') {
        return from_ct.to_string();
    }
    // Fall back to a known extension in the URL path.
    let path = url.split(['?', '#']).next().unwrap_or(url);
    if let Some(dot) = path.rfind('.') {
        let e = path[dot + 1..].to_lowercase();
        if matches!(
            e.as_str(),
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "avif" | "bmp"
        ) {
            return if e == "jpeg" { "jpg".into() } else { e };
        }
    }
    String::new()
}

/// Deletes least-recently-modified files until the cache is under budget.
/// Runs the directory walk off the blocking pool.
async fn evict_if_needed(dir: &Path) -> std::io::Result<()> {
    let dir = dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(());
        };
        let mut total = 0u64;
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(md) = std::fs::metadata(&path) {
                if !md.is_file() {
                    continue;
                }
                let size = md.len();
                let mtime = md.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                // Skip in-progress temp files from eviction accounting.
                if path.extension().and_then(|e| e.to_str()) == Some("tmp") {
                    continue;
                }
                total += size;
                files.push((mtime, size, path));
            }
        }
        if total <= MAX_CACHE_BYTES {
            return Ok(());
        }
        files.sort_by_key(|(mtime, _, _)| *mtime);
        for (_, size, path) in files {
            if total <= MAX_CACHE_BYTES {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
        Ok(())
    })
    .await
    .unwrap_or(Ok(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_stable_and_64_chars() {
        let a = url_hex("https://x/a.png");
        let b = url_hex("https://x/a.png");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert_ne!(a, url_hex("https://x/b.png"));
    }

    #[test]
    fn extension_prefers_content_type_then_url() {
        assert_eq!(extension_for("image/jpeg", "https://x/y"), "jpg");
        assert_eq!(
            extension_for("image/png; charset=binary", "https://x/y"),
            "png"
        );
        assert_eq!(extension_for("text/html", "https://x/y.WEBP"), "webp");
        // Unknown type and no image extension -> empty (skip caching).
        assert_eq!(extension_for("text/html", "https://x/y"), "");
    }

    #[test]
    fn scan_maps_existing_by_hex_stem() {
        let dir = tempfile::tempdir().unwrap();
        let hex = url_hex("https://x/a.png");
        std::fs::write(dir.path().join(format!("{hex}.png")), b"hi").unwrap();
        std::fs::write(dir.path().join("notahashfile.png"), b"x").unwrap();
        let map = scan_existing(dir.path());
        assert!(map.contains_key(&hex));
        assert_eq!(map.len(), 1);
    }
}
