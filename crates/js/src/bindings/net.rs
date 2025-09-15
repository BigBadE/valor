use crate::bindings::util::{apply_headers_from_json, collect_headers, maybe_attach_body};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use bytes::Bytes;
use reqwest::{Client, Method};
use url::Url;

use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct FetchDone {
    pub status: u16,
    pub status_text: String,
    pub ok: bool,
    pub headers: Vec<(String, String)>,
    pub body_text: String,
    pub body_b64: String,
    pub url: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub enum FetchEntry {
    Pending,
    Done(FetchDone),
}

#[derive(Debug, Default)]
pub struct FetchRegistry {
    pub next_id: u64,
    pub entries: HashMap<u64, FetchEntry>,
}

impl FetchRegistry {
    pub fn allocate_id(&mut self) -> u64 {
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.next_id
    }
}

pub async fn fetch_file(parsed: &Url, url_final: String) -> Result<FetchDone, String> {
    let path = parsed
        .to_file_path()
        .map_err(|_| String::from("Invalid file path"))?;
    let data = tokio::fs::read(path)
        .await
        .map_err(|_| String::from("File read error"))?;
    let bytes = Bytes::from(data);
    let body_text = String::from_utf8_lossy(&bytes).to_string();
    let body_b64 = BASE64_STANDARD.encode(bytes);
    Ok(FetchDone {
        status: 200,
        status_text: String::from("OK"),
        ok: true,
        headers: Vec::new(),
        body_text,
        body_b64,
        url: url_final,
        error: None,
    })
}

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
    if let Some(hs) = headers_json.as_ref() {
        req = apply_headers_from_json(req, hs);
    }
    req = maybe_attach_body(req, &body_b64);

    let resp = req
        .send()
        .await
        .map_err(|_| String::from("Network error"))?;
    let status = resp.status().as_u16();
    let status_text = resp.status().canonical_reason().unwrap_or("").to_string();
    let headers = collect_headers(&resp);
    let bytes = resp
        .bytes()
        .await
        .map_err(|_| String::from("Read body error"))?;
    let body_text = String::from_utf8_lossy(&bytes).to_string();
    let body_b64 = BASE64_STANDARD.encode(bytes);
    Ok(FetchDone {
        status,
        status_text,
        ok: (200..300).contains(&status),
        headers,
        body_text,
        body_b64,
        url: url_final,
        error: None,
    })
}
