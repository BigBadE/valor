//! Type conversions between JSValue and V8 values.

use js::JSValue;
use v8::{Boolean, Local, Number, String as V8String, Value};

/// Escape a string so it can safely be embedded as a JavaScript literal.
#[inline]
pub(crate) fn escape_js_for_literal(input: &str) -> String {
    let mut out = String::with_capacity(input.len().saturating_add(8));
    for character in input.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(character),
        }
    }
    out
}

/// Produce a small script that calls `window.onerror(message, url, 0, 0)` if available.
#[inline]
pub(crate) fn build_onerror_script(message: &str, url: &str) -> String {
    let msg_lit = format!("\"{}\"", escape_js_for_literal(message));
    let url_lit = format!("\"{}\"", escape_js_for_literal(url));
    format!(
        "(function(m,u){{try{{if(typeof window!=='undefined'&&typeof window.onerror==='function'){{window.onerror(m,u,0,0);}}}}catch(_o){{}}}})({msg_lit},{url_lit});"
    )
}

/// Collect V8 callback arguments into engine-agnostic `JSValue`s.
#[inline]
pub(crate) fn collect_js_args<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: &v8::FunctionCallbackArguments,
) -> Vec<JSValue> {
    let mut collected: Vec<JSValue> = Vec::new();
    let length = args.length();
    let start: i32 = 0;
    for index in start..length {
        let value = args.get(index);
        if value.is_undefined() {
            collected.push(JSValue::Undefined);
            continue;
        }
        if value.is_null() {
            collected.push(JSValue::Null);
            continue;
        }
        if value.is_boolean() {
            collected.push(JSValue::Boolean(value.boolean_value(scope)));
            continue;
        }
        if value.is_number() {
            collected.push(JSValue::Number(
                value.number_value(scope).unwrap_or(f64::NAN),
            ));
            continue;
        }
        if value.is_string() {
            collected.push(JSValue::String(
                value
                    .to_string(scope)
                    .map_or_else(String::new, |val_str| val_str.to_rust_string_lossy(scope)),
            ));
            continue;
        }
        let stringified = value.to_string(scope).map_or_else(
            || String::from("undefined"),
            |val_str| val_str.to_rust_string_lossy(scope),
        );
        collected.push(JSValue::String(stringified));
    }
    collected
}

#[inline]
/// Convert a `JSValue` into a V8 `Local<Value>`.
///
/// # Arguments
///
/// * `scope`: The V8 handle scope.
/// * `value`: The `JSValue` to convert.
///
/// # Returns
/// The converted `Local<Value>`.
pub(crate) fn jsvalue_to_local<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    value: JSValue,
) -> Local<'s, Value> {
    match value {
        JSValue::Undefined => v8::undefined(scope).into(),
        JSValue::Null => v8::null(scope).into(),
        JSValue::Boolean(boolean_value) => Boolean::new(scope, boolean_value).into(),
        JSValue::Number(number_value) => Number::new(scope, number_value).into(),
        JSValue::String(string_value) => V8String::new(scope, string_value.as_str())
            .map_or_else(|| v8::undefined(scope).into(), Into::into),
    }
}
