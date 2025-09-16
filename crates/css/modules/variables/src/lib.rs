//! CSS Custom Properties for Cascading Variables Module Level 1 — CSS variables.
//! Spec: <https://www.w3.org/TR/css-variables-1/>

#![forbid(unsafe_code)]

use core::hash::BuildHasher;
use std::collections::HashMap;

/// Alias used by helpers that operate on a set of custom properties.
/// Keys are property names (including the leading `--`); values are raw token strings.
pub type CustomProperties = HashMap<String, String>;

/// Extract custom properties (`--*`) from a declaration map.
///
/// This is a simple filter that keeps only entries whose property name begins with `--`.
/// It can be used on inline style maps or computed declaration maps to produce a
/// custom properties environment for `var()` resolution.
///
/// Spec: <https://www.w3.org/TR/css-variables-1/#custom-properties>
#[inline]
pub fn extract_custom_properties<S: BuildHasher + Default>(
    declarations: &HashMap<String, String, S>,
) -> CustomProperties {
    let mut out: CustomProperties = HashMap::default();
    // Avoid iterating unordered maps directly; build a sorted list of keys for determinism.
    let mut keys: Vec<&String> = declarations.keys().collect();
    keys.sort();
    for key in keys {
        if key.starts_with("--")
            && let Some(value) = declarations.get(key)
        {
            out.insert(key.clone(), value.clone());
        }
    }
    out
}

/// Resolve `var()` functions within a value against the provided custom properties.
///
/// This implements a conservative MVP subset:
/// - Supports `var(--name)` and `var(--name, fallback)`.
/// - Looks up `--name` first in `inherited` (parent) then in `current` if not found in parent.
///   This approximates inheritance behavior for custom properties.
/// - Performs recursive expansion to resolve nested `var()` inside referenced values or fallbacks.
/// - Basic cycle detection: if a variable references itself directly or indirectly, the reference
///   is treated as invalid; if a fallback is provided, it is used, otherwise the reference is
///   replaced with the empty string.
/// - Parsing is string-based and tolerant. If a `var(` has no closing `)`, its tail is preserved
///   as-is after resolving the leading segment.
///
/// Note: This routine does not perform tokenization or validate tokens per the Syntax spec.
/// It is intentionally small for early wiring and can be replaced by a tokenizer-backed
/// resolver later without changing the signature.
///
/// Spec: <https://www.w3.org/TR/css-variables-1/#using-variables>
#[inline]
pub fn resolve_vars_in_value(
    value_text: &str,
    current: &CustomProperties,
    inherited: &CustomProperties,
) -> String {
    resolve_vars_internal(value_text, current, inherited, &mut Vec::new())
}

/// Internal recursive resolver that carries the resolution stack for cycle detection.
///
/// Limitations: does not handle nested parentheses inside the argument list beyond the
/// first closing `)`. Nested `var()` within fallbacks are still handled since fallback
/// resolution recurses separately.
///
/// Spec: <https://www.w3.org/TR/css-variables-1/#cycles>
#[inline]
fn resolve_vars_internal(
    value_text: &str,
    current: &CustomProperties,
    inherited: &CustomProperties,
    stack: &mut Vec<String>,
) -> String {
    if let Some((head, after_open)) = value_text.split_once("var(") {
        // If no closing paren, preserve the tail as-is after resolving the head.
        let Some((args_text, tail)) = after_open.split_once(')') else {
            return [head, "var(", after_open].concat();
        };
        let replacement = resolve_single_var(args_text, current, inherited, stack);
        let resolved_tail = resolve_vars_internal(tail, current, inherited, stack);
        return [head, &replacement, &resolved_tail].concat();
    }
    value_text.to_owned()
}

/// Resolve a single `var()` argument string like `--name` or `--name, fallback`.
/// Returns the resolved string, possibly expanding nested vars.
///
/// Spec: <https://www.w3.org/TR/css-variables-1/#using-variables>
#[inline]
fn resolve_single_var(
    args_text: &str,
    current: &CustomProperties,
    inherited: &CustomProperties,
    stack: &mut Vec<String>,
) -> String {
    let (name_text_raw, fallback_text_raw) = match args_text.split_once(',') {
        Some((first, second)) => (first.trim(), Some(second.trim())),
        None => (args_text.trim(), None),
    };
    let name_text = name_text_raw;
    if !name_text.starts_with("--") {
        // Invalid custom property name, use fallback or empty.
        return fallback_text_raw.map_or_else(String::new, |fallback_src| {
            resolve_vars_internal(fallback_src, current, inherited, stack)
        });
    }

    // Choose value from inherited first, then current scope.
    let candidate_value = inherited
        .get(name_text)
        .or_else(|| current.get(name_text))
        .cloned();

    match candidate_value {
        Some(resolved_value) => {
            // Cycle detection
            if stack.contains(&name_text.to_owned()) {
                return fallback_text_raw.map_or_else(String::new, |fallback_src| {
                    resolve_vars_internal(fallback_src, current, inherited, stack)
                });
            }
            stack.push(name_text.to_owned());
            let expanded = resolve_vars_internal(&resolved_value, current, inherited, stack);
            stack.pop();
            expanded
        }
        None => fallback_text_raw.map_or_else(String::new, |fallback_src| {
            resolve_vars_internal(fallback_src, current, inherited, stack)
        }),
    }
}
