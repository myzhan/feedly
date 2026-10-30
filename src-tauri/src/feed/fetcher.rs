use futures::StreamExt;
use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use reqwest::{Client, Response, StatusCode};

use crate::error::{AppError, AppResult};

/// Hard cap on how many bytes we will buffer for a single feed response.
const MAX_BYTES: usize = 5 * 1024 * 1024;

pub enum FetchOutcome {
    /// Server honoured conditional headers and reported no change.
    NotModified,
    /// Fresh body bytes plus any new validator headers to persist.
    Updated {
        bytes: Vec<u8>,
        etag: Option<String>,
        last_modified: Option<String>,
    },
}

fn header(resp: &Response, name: reqwest::header::HeaderName) -> Option<String> {
    resp.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Streams the response body, aborting if it exceeds `MAX_BYTES`.
async fn read_capped(resp: Response) -> AppResult<Vec<u8>> {
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if buf.len() + chunk.len() > MAX_BYTES {
            return Err(AppError::Network("响应体超过 5MB 上限，已中止".into()));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// Performs an HTTP GET with optional conditional-request validators.
pub async fn fetch(
    client: &Client,
    url: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> AppResult<FetchOutcome> {
    let mut req = client.get(url);
    if let Some(e) = etag {
        req = req.header(IF_NONE_MATCH, e);
    }
    if let Some(lm) = last_modified {
        req = req.header(IF_MODIFIED_SINCE, lm);
    }

    let resp = req.send().await?;
    if resp.status() == StatusCode::NOT_MODIFIED {
        return Ok(FetchOutcome::NotModified);
    }
    let resp = resp.error_for_status()?;

    let new_etag = header(&resp, ETAG);
    let new_lm = header(&resp, LAST_MODIFIED);
    let bytes = read_capped(resp).await?;
    Ok(FetchOutcome::Updated {
        bytes,
        etag: new_etag,
        last_modified: new_lm,
    })
}
