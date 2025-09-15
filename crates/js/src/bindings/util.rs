use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use reqwest::RequestBuilder;
use reqwest::Response;
use serde_json::Value;

#[inline]
pub fn apply_headers_from_json(mut req: RequestBuilder, headers_json: &str) -> RequestBuilder {
    if let Ok(val) = serde_json::from_str::<Value>(headers_json) {
        if let Some(map) = val.as_object() {
            for (k, v) in map {
                if let Some(s) = v.as_str() {
                    req = req.header(k, s);
                }
            }
            return req;
        }
        if let Some(arr) = val.as_array() {
            for pair in arr {
                let Some(k) = pair.get(0).and_then(Value::as_str) else {
                    continue;
                };
                let Some(v) = pair.get(1).and_then(Value::as_str) else {
                    continue;
                };
                req = req.header(k, v);
            }
        }
    }
    req
}

#[inline]
pub fn collect_headers(resp: &Response) -> Vec<(String, String)> {
    resp.headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.to_string(), v.to_string()))
        })
        .collect()
}

#[inline]
pub fn maybe_attach_body(req: RequestBuilder, body_b64: &Option<String>) -> RequestBuilder {
    match body_b64 {
        Some(b64) if !b64.is_empty() => match BASE64_STANDARD.decode(b64) {
            Ok(bytes_vec) => req.body(bytes_vec),
            Err(_) => req,
        },
        _ => req,
    }
}
