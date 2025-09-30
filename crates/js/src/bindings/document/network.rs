//! Network request functions for async HTTP/file fetching.

use crate::bindings::document_helpers::parse_net_request_args;
use crate::bindings::values::{JSError, JSValue};
use crate::bindings::{HostContext, HostFnSync};
use std::sync::Arc;
/// Build `net_request` function - starts an async network request.
#[inline]
pub fn build_net_request() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            use crate::bindings::net::{fetch_file, fetch_http, FetchDone, FetchEntry};
            use std::env;
            use url::Url;

            let (method, url_str, headers_json, body_b64) = parse_net_request_args(&args)?;

            let relaxed = env::var("VALOR_NET_RELAXED")
                .ok()
                .is_some_and(|val| val == "1" || val.eq_ignore_ascii_case("true"));
            let parsed = Url::parse(&url_str)
                .map_err(|_| JSError::TypeError(format!("invalid URL: {url_str}")))?;
            let allowed = relaxed
                || matches!(parsed.scheme(), "file")
                || (parsed.scheme() == "http"
                    && parsed.host_str().is_some_and(|host| {
                        host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1"
                    }));
            let chrome_restricted = context.page_origin.starts_with("valor://chrome")
                && matches!(parsed.scheme(), "http" | "https");

            let id = {
                let mut reg = context
                    .fetch_registry
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
                let id = reg.allocate_id();
                reg.entries.insert(id, FetchEntry::Pending);
                id
            };

            let reg_arc = Arc::clone(&context.fetch_registry);
            let (method_upper, url_clone, url_for_error) =
                (method.to_ascii_uppercase(), url_str.clone(), url_str);

            context.tokio_handle.spawn({
                let finalize = move |done: FetchDone| {
                    if let Ok(mut reg) = reg_arc.lock() {
                        reg.entries.insert(id, FetchEntry::Done(done));
                    }
                };
                let err_resp = move |error: String| FetchDone {
                    status: 0,
                    status_text: String::new(),
                    is_ok: false,
                    headers: Vec::new(),
                    body_text: String::new(),
                    body_b64: String::new(),
                    url: url_for_error.clone(),
                    error: Some(error),
                };
                async move {
                    if !allowed || chrome_restricted {
                        finalize(err_resp(String::from("Disallowed by policy")));
                        return;
                    }
                    let result = match parsed.scheme() {
                        "file" => fetch_file(&parsed, url_clone.clone()).await,
                        "http" | "https" => {
                            fetch_http(&method_upper, url_clone, headers_json, body_b64).await
                        }
                        scheme => Err(format!("Unsupported scheme: {scheme}")),
                    };
                    finalize(result.unwrap_or_else(err_resp));
                }
            });

            Ok(JSValue::String(id.to_string()))
        },
    )
}

/// Build `net_requestPoll` function - polls the status of an async network request.
#[inline]
pub fn build_net_request_poll() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            use crate::bindings::net::FetchEntry;

            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "net_requestPoll(id) requires 1 argument",
                )));
            }
            let id: u64 = match &args[0] {
                JSValue::String(string_value) => string_value
                    .parse::<u64>()
                    .map_err(|_| JSError::TypeError(String::from("invalid id")))?,
                _ => return Err(JSError::TypeError(String::from("id must be string"))),
            };
            let reg = context
                .fetch_registry
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            let json = match reg.entries.get(&id) {
                None => serde_json::json!({"state":"error","error":"unknown id"}).to_string(),
                Some(FetchEntry::Pending) => serde_json::json!({"state":"pending"}).to_string(),
                Some(FetchEntry::Done(done)) => serde_json::json!({
                    "state":"done",
                    "status": done.status,
                    "statusText": done.status_text,
                    "ok": done.is_ok,
                    "headers": done.headers,
                    "bodyText": done.body_text,
                    "bodyBase64": done.body_b64,
                    "url": done.url,
                    "error": done.error
                })
                .to_string(),
            };
            Ok(JSValue::String(json))
        },
    )
}
