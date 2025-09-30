use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use reqwest::RequestBuilder;
use reqwest::Response;
use serde_json::Value;

/// Apply HTTP headers from a JSON string to a request builder.
/// Supports both object format `{"key": "value"}` and array format `[["key", "value"]]`.
#[inline]
pub fn apply_headers_from_json(mut req: RequestBuilder, headers_json: &str) -> RequestBuilder {
    if let Ok(val) = serde_json::from_str::<Value>(headers_json) {
        if let Some(map) = val.as_object() {
            for (key, value) in map {
                if let Some(string_value) = value.as_str() {
                    req = req.header(key, string_value);
                }
            }
            return req;
        }
        if let Some(arr) = val.as_array() {
            for pair in arr {
                let Some(key) = pair.get(0).and_then(Value::as_str) else {
                    continue;
                };
                let Some(value) = pair.get(1).and_then(Value::as_str) else {
                    continue;
                };
                req = req.header(key, value);
            }
        }
    }
    req
}

/// Collect all HTTP headers from a response into a vector of (name, value) pairs.
#[inline]
pub fn collect_headers(resp: &Response) -> Vec<(String, String)> {
    resp.headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value_str| (name.to_string(), value_str.to_owned()))
        })
        .collect()
}

/// Attach a base64-encoded body to a request builder if present and non-empty.
#[inline]
pub fn maybe_attach_body(req: RequestBuilder, body_b64: Option<&String>) -> RequestBuilder {
    match body_b64 {
        Some(base64_string) if !base64_string.is_empty() => {
            match BASE64_STANDARD.decode(base64_string) {
                Ok(bytes_vec) => req.body(bytes_vec),
                Err(_) => req,
            }
        }
        _ => req,
    }
}
