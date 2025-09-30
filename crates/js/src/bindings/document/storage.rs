//! Storage functions for localStorage and sessionStorage.

use crate::bindings::values::{JSError, JSValue};
use crate::bindings::{HostContext, HostFnSync};
use std::sync::Arc;
/// Build `storage_getItem` function.
#[inline]
pub fn build_storage_get_item() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.len() < 2 {
                return Err(JSError::TypeError(String::from(
                    "storage_getItem(kind, key) requires 2 arguments",
                )));
            }
            let kind = match &args[0] {
                JSValue::String(string_value) => string_value.as_str(),
                _ => return Err(JSError::TypeError(String::from("kind must be string"))),
            };
            let key = match &args[1] {
                JSValue::String(string_value) => string_value.clone(),
                _ => return Err(JSError::TypeError(String::from("key must be string"))),
            };
            let origin = context.page_origin.clone();
            let value = match kind {
                "local" => context
                    .storage_local
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?
                    .get_bucket(&origin)
                    .and_then(|bucket| bucket.get(&key).cloned())
                    .unwrap_or_default(),
                "session" => context
                    .storage_session
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?
                    .get_bucket(&origin)
                    .and_then(|bucket| bucket.get(&key).cloned())
                    .unwrap_or_default(),
                _ => String::new(),
            };
            Ok(JSValue::String(value))
        },
    )
}

/// Build `storage_setItem` function.
#[inline]
pub fn build_storage_set_item() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.len() < 3 {
                return Err(JSError::TypeError(String::from(
                    "storage_setItem(kind, key, value) requires 3 arguments",
                )));
            }
            let kind = match &args[0] {
                JSValue::String(string_value) => string_value.as_str(),
                _ => return Err(JSError::TypeError(String::from("kind must be string"))),
            };
            let key = match &args[1] {
                JSValue::String(string_value) => string_value.clone(),
                _ => return Err(JSError::TypeError(String::from("key must be string"))),
            };
            let value = match &args[2] {
                JSValue::String(string_value) => string_value.clone(),
                _ => return Err(JSError::TypeError(String::from("value must be string"))),
            };
            let origin = context.page_origin.clone();
            match kind {
                "local" => {
                    let mut reg = context
                        .storage_local
                        .lock()
                        .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
                    reg.get_bucket_mut(&origin).insert(key, value);
                }
                "session" => {
                    let mut reg = context
                        .storage_session
                        .lock()
                        .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
                    reg.get_bucket_mut(&origin).insert(key, value);
                }
                _ => {}
            }
            Ok(JSValue::Undefined)
        },
    )
}

/// Build `storage_removeItem` function.
#[inline]
pub fn build_storage_remove_item() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.len() < 2 {
                return Err(JSError::TypeError(String::from(
                    "storage_removeItem(kind, key) requires 2 arguments",
                )));
            }
            let kind = match &args[0] {
                JSValue::String(string_value) => string_value.as_str(),
                _ => return Err(JSError::TypeError(String::from("kind must be string"))),
            };
            let key = match &args[1] {
                JSValue::String(string_value) => string_value.clone(),
                _ => return Err(JSError::TypeError(String::from("key must be string"))),
            };
            let origin = context.page_origin.clone();
            match kind {
                "local" => {
                    if let Ok(mut reg) = context.storage_local.lock() {
                        if let Some(bucket) = reg.buckets.get_mut(&origin) {
                            bucket.remove(&key);
                        }
                    }
                }
                "session" => {
                    if let Ok(mut reg) = context.storage_session.lock() {
                        if let Some(bucket) = reg.buckets.get_mut(&origin) {
                            bucket.remove(&key);
                        }
                    }
                }
                _ => {}
            }
            Ok(JSValue::Undefined)
        },
    )
}

/// Build `storage_clear` function.
#[inline]
pub fn build_storage_clear() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "storage_clear(kind) requires 1 argument",
                )));
            }
            let kind = match &args[0] {
                JSValue::String(string_value) => string_value.as_str(),
                _ => return Err(JSError::TypeError(String::from("kind must be string"))),
            };
            let origin = context.page_origin.clone();
            match kind {
                "local" => {
                    if let Ok(mut reg) = context.storage_local.lock() {
                        if let Some(bucket) = reg.buckets.get_mut(&origin) {
                            bucket.clear();
                        }
                    }
                }
                "session" => {
                    if let Ok(mut reg) = context.storage_session.lock() {
                        if let Some(bucket) = reg.buckets.get_mut(&origin) {
                            bucket.clear();
                        }
                    }
                }
                _ => {}
            }
            Ok(JSValue::Undefined)
        },
    )
}

/// Build `storage_keys` function.
#[inline]
pub fn build_storage_keys() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "storage_keys(kind) requires 1 argument",
                )));
            }
            let kind = match &args[0] {
                JSValue::String(string_value) => string_value.as_str(),
                _ => return Err(JSError::TypeError(String::from("kind must be string"))),
            };
            let origin = context.page_origin.clone();
            let keys: Vec<String> = match kind {
                "local" => context
                    .storage_local
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?
                    .get_bucket(&origin)
                    .map(|bucket| bucket.keys().cloned().collect())
                    .unwrap_or_default(),
                "session" => context
                    .storage_session
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?
                    .get_bucket(&origin)
                    .map(|bucket| bucket.keys().cloned().collect())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            Ok(JSValue::String(keys.join(" ")))
        },
    )
}
