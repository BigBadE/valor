use crate::bindings::util::{collect_headers, maybe_attach_body};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use bytes::Bytes;
use reqwest::{Client, Method};
use tokio::fs::read as tokio_read;
use url::Url;

use std::collections::HashMap;

/// Result of a completed fetch request.
#[derive(Clone, Debug)]
pub struct FetchDone {
    /// HTTP status code (or 200 for file://).
    pub status: u16,
    /// HTTP status text.
    pub status_text: String,
    /// Whether the request was successful (2xx status).
    pub is_ok: bool,
    /// Response headers as (name, value) pairs.
    pub headers: Vec<(String, String)>,
    /// Response body as UTF-8 text.
    pub body_text: String,
    /// Response body as base64-encoded string.
    pub body_b64: String,
    /// Final URL after redirects.
    pub url: String,
    /// Error message if request failed.
    pub error: Option<String>,
}

/// Registry for tracking async network fetch requests.
#[derive(Clone, Debug)]
pub struct FetchRegistry {
    /// Next ID to allocate for a fetch request.
    pub next_id: u64,
    /// Map of fetch request IDs to their current state.
    pub entries: HashMap<u64, FetchEntry>,
}

/// State of a fetch request.
#[derive(Clone, Debug)]
pub enum FetchEntry {
    /// Request is pending.
    Pending,
    /// Request completed with result.
    Done(FetchDone),
}

impl FetchRegistry {
    /// Allocate a new unique ID for a fetch request.
    pub fn allocate_id(&mut self) -> u64 {
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.next_id
    }
}

/// Fetch a file:// URL resource.
///
/// # Errors
/// Returns an error string if the file path is invalid or cannot be read.
pub async fn fetch_file(parsed: &Url, url_final: String) -> Result<FetchDone, String> {
    let path = parsed
        .to_file_path()
        .map_err(|()| String::from("Invalid file path"))?;
    let data = tokio_read(path)
        .await
        .map_err(|_| String::from("File read error"))?;
    let bytes = Bytes::from(data);
    let body_text = String::from_utf8_lossy(&bytes).to_string();
    let body_base64 = BASE64_STANDARD.encode(bytes);
    Ok(FetchDone {
        status: 200,
        status_text: String::from("OK"),
        is_ok: true,
        headers: Vec::new(),
        body_text,
        body_b64: body_base64,
        url: url_final,
        error: None,
    })
}

/// Fetch an HTTP(S) resource.
///
/// # Errors
/// Returns an error string if the request fails or response cannot be read.
pub async fn fetch_http(
    method_upper: &str,
    url_final: String,
    headers_json: Option<String>,
    body_b64: Option<String>,
) -> Result<FetchDone, String> {
    let client = Client::new();
    let mut req = client.request(
        Method::from_bytes(method_upper.as_bytes()).unwrap_or(Method::GET),
        url_final.clone(),
    );
    if let Some(headers_str) = headers_json.as_ref() {
        req = super::util::apply_headers_from_json(req, headers_str);
    }
    req = maybe_attach_body(req, body_b64.as_ref());

    let resp = req
        .send()
        .await
        .map_err(|_| String::from("Network error"))?;
    let status = resp.status().as_u16();
    let status_text = resp.status().canonical_reason().unwrap_or("").to_owned();
    let headers = collect_headers(&resp);
    let bytes = resp
        .bytes()
        .await
        .map_err(|_| String::from("Read body error"))?;
    let body_text = String::from_utf8_lossy(&bytes).to_string();
    let body_base64 = BASE64_STANDARD.encode(bytes);
    Ok(FetchDone {
        status,
        status_text,
        is_ok: (200..300).contains(&status),
        headers,
        body_text,
        body_b64: body_base64,
        url: url_final,
        error: None,
    })
}
