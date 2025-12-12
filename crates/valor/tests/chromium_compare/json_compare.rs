use serde_json::{Number as JsonNumber, Value as JsonValue, value::Map as JsonMap};
use std::fmt::Write as _;

fn extract_id_label(value: &JsonValue) -> Option<String> {
    let JsonValue::Object(map) = value else {
        return None;
    };

    if let Some(JsonValue::Object(attrs)) = map.get("attrs")
        && let Some(JsonValue::String(id)) = attrs.get("id")
    {
        return Some(format!("#{id}"));
    }

    if let Some(JsonValue::String(id)) = map.get("id") {
        return Some(format!("#{id}"));
    }

    None
}

fn is_element_object(value: &JsonValue) -> bool {
    if let JsonValue::Object(map) = value {
        map.contains_key("tag") && map.contains_key("rect")
    } else {
        false
    }
}

fn get_element_tag(value: &JsonValue) -> Option<String> {
    if let JsonValue::Object(map) = value {
        map.get("tag")
            .and_then(|tag_value| tag_value.as_str())
            .map(ToString::to_string)
    } else {
        None
    }
}

fn format_path(path: &[String]) -> String {
    if path.is_empty() {
        String::new()
    } else {
        let joined = path.join("");
        joined
            .strip_prefix('.')
            .map_or_else(|| joined.clone(), ToString::to_string)
    }
}

fn type_name(json_value: &JsonValue) -> &'static str {
    match json_value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

#[derive(Debug, Clone)]
struct Difference {
    path: String,
    element_path: String,
    actual: String,
    expected: String,
}

struct CompareParams<'cmp> {
    eps: f64,
    path: &'cmp mut Vec<String>,
    elem_path: &'cmp mut Vec<String>,
    diffs: &'cmp mut Vec<Difference>,
}

type HelperFn = fn(&JsonValue, &JsonValue, &mut CompareParams<'_>);

struct CompareContext<'cmp> {
    eps: f64,
    path: &'cmp mut Vec<String>,
    elem_path: &'cmp mut Vec<String>,
    diffs: &'cmp mut Vec<Difference>,
    helper: HelperFn,
}

struct NumberCompareParams<'num> {
    actual: &'num JsonNumber,
    expected: &'num JsonNumber,
    eps: f64,
    path: &'num [String],
    elem_path: &'num [String],
    diffs: &'num mut Vec<Difference>,
}

/// Compares two JSON numbers with an epsilon tolerance.
fn compare_numbers(params: &mut NumberCompareParams<'_>) {
    match (params.actual.as_f64(), params.expected.as_f64()) {
        (Some(actual_float), Some(expected_float)) => {
            if (actual_float - expected_float).abs() > params.eps {
                params.diffs.push(Difference {
                    path: format_path(params.path),
                    element_path: params.elem_path.join(" > "),
                    actual: actual_float.to_string(),
                    expected: expected_float.to_string(),
                });
            }
        }
        _ => {
            params.diffs.push(Difference {
                path: format_path(params.path),
                element_path: params.elem_path.join(" > "),
                actual: params.actual.to_string(),
                expected: params.expected.to_string(),
            });
        }
    }
}

/// Compares two JSON arrays element by element.
fn compare_arrays(
    actual_arr: &[JsonValue],
    expected_arr: &[JsonValue],
    ctx: &mut CompareContext<'_>,
) {
    if actual_arr.len() != expected_arr.len() {
        ctx.diffs.push(Difference {
            path: format_path(ctx.path),
            element_path: ctx.elem_path.join(" > "),
            actual: actual_arr.len().to_string(),
            expected: expected_arr.len().to_string(),
        });
        return;
    }

    for (index, (actual_item, expected_item)) in
        actual_arr.iter().zip(expected_arr.iter()).enumerate()
    {
        let is_children_ctx = ctx
            .path
            .last()
            .is_some_and(|segment| segment == ".children");
        if is_children_ctx {
            let label = extract_id_label(actual_item)
                .or_else(|| extract_id_label(expected_item))
                .unwrap_or_else(|| index.to_string());
            ctx.path.pop();
            ctx.path.push(format!(".{label}"));
        } else {
            ctx.path.push(format!("[{index}]"));
        }

        let should_push = is_element_object(actual_item) && is_element_object(expected_item);
        if should_push {
            let tag = get_element_tag(actual_item)
                .or_else(|| get_element_tag(expected_item))
                .unwrap_or_else(|| "?".to_string());
            let id = extract_id_label(actual_item)
                .or_else(|| extract_id_label(expected_item))
                .unwrap_or_else(|| format!("[{index}]"));
            ctx.elem_path.push(format!("{tag}{id}"));
        }

        (ctx.helper)(
            actual_item,
            expected_item,
            &mut CompareParams {
                eps: ctx.eps,
                path: ctx.path,
                elem_path: ctx.elem_path,
                diffs: ctx.diffs,
            },
        );

        if should_push {
            ctx.elem_path.pop();
        }
        ctx.path.pop();
    }
}

/// Compares two JSON objects key by key.
fn compare_objects(
    actual_obj: &JsonMap<String, JsonValue>,
    expected_obj: &JsonMap<String, JsonValue>,
    ctx: &mut CompareContext<'_>,
) {
    // Count non-fontFamily keys for size comparison
    let actual_count = actual_obj
        .keys()
        .filter(|key| key.as_str() != "fontFamily")
        .count();
    let expected_count = expected_obj
        .keys()
        .filter(|key| key.as_str() != "fontFamily")
        .count();

    if actual_count != expected_count {
        ctx.diffs.push(Difference {
            path: format_path(ctx.path),
            element_path: ctx.elem_path.join(" > "),
            actual: actual_count.to_string(),
            expected: expected_count.to_string(),
        });
    }

    for (key, actual_val) in actual_obj {
        // Skip fontFamily comparison - it's a text rendering concern, not layout
        if key == "fontFamily" {
            continue;
        }

        match expected_obj.get(key) {
            Some(expected_val) => {
                ctx.path.push(format!(".{key}"));
                let should_push = is_element_object(actual_val) && is_element_object(expected_val);
                if should_push {
                    let tag = get_element_tag(actual_val)
                        .or_else(|| get_element_tag(expected_val))
                        .unwrap_or_else(|| "?".to_string());
                    let id = extract_id_label(actual_val)
                        .or_else(|| extract_id_label(expected_val))
                        .unwrap_or_else(|| key.clone());
                    ctx.elem_path.push(format!("{tag}#{id}"));
                }

                (ctx.helper)(
                    actual_val,
                    expected_val,
                    &mut CompareParams {
                        eps: ctx.eps,
                        path: ctx.path,
                        elem_path: ctx.elem_path,
                        diffs: ctx.diffs,
                    },
                );

                if should_push {
                    ctx.elem_path.pop();
                }
                ctx.path.pop();
            }
            None => {
                ctx.diffs.push(Difference {
                    path: format_path(ctx.path),
                    element_path: ctx.elem_path.join(" > "),
                    actual: format!("has '{key}'"),
                    expected: "missing".to_string(),
                });
            }
        }
    }
}

fn should_skip_root_rect_dimension(elem_path: &[String], path: &[String]) -> bool {
    elem_path.len() <= 1 && path.len() >= 2 && {
        let (last, prev) = (&path[path.len() - 1], &path[path.len() - 2]);
        matches!(last.as_str(), ".width" | ".height") && prev == ".rect"
    }
}

fn is_rect_dimension(path: &[String]) -> bool {
    path.len() >= 2 && {
        let last = &path[path.len() - 1];
        let prev = &path[path.len() - 2];
        matches!(last.as_str(), ".width" | ".height") && prev == ".rect"
    }
}

fn compare_primitives(
    actual_value: &JsonValue,
    expected_value: &JsonValue,
    path: &[String],
    elem_path: &[String],
    diffs: &mut Vec<Difference>,
) {
    match (actual_value, expected_value) {
        (JsonValue::Bool(actual_bool), JsonValue::Bool(expected_bool))
            if actual_bool != expected_bool =>
        {
            diffs.push(Difference {
                path: format_path(path),
                element_path: elem_path.join(" > "),
                actual: actual_bool.to_string(),
                expected: expected_bool.to_string(),
            });
        }
        (JsonValue::String(actual_str), JsonValue::String(expected_str))
            if actual_str != expected_str =>
        {
            diffs.push(Difference {
                path: format_path(path),
                element_path: elem_path.join(" > "),
                actual: actual_str.clone(),
                expected: expected_str.clone(),
            });
        }
        _ => {}
    }
}

/// Helper function to recursively compare JSON values.
fn compare_json_helper(
    actual_value: &JsonValue,
    expected_value: &JsonValue,
    params: &mut CompareParams<'_>,
) {
    // Special-case: ignore root-level rect width/height diffs
    if should_skip_root_rect_dimension(params.elem_path, params.path) {
        return;
    }

    // Use small epsilon (0.02px) for rect dimensions, strict (eps=0) for everything else
    let effective_eps = if is_rect_dimension(params.path) {
        0.02
    } else {
        params.eps
    };

    match (actual_value, expected_value) {
        (JsonValue::Null, JsonValue::Null) => {}
        (JsonValue::Bool(actual_bool), JsonValue::Bool(expected_bool))
            if actual_bool == expected_bool => {}
        (JsonValue::String(actual_str), JsonValue::String(expected_str))
            if actual_str == expected_str => {}
        (JsonValue::Number(actual_num), JsonValue::Number(expected_num)) => {
            compare_numbers(&mut NumberCompareParams {
                actual: actual_num,
                expected: expected_num,
                eps: effective_eps,
                path: params.path,
                elem_path: params.elem_path,
                diffs: params.diffs,
            });
        }
        (JsonValue::Array(actual_arr), JsonValue::Array(expected_arr)) => {
            compare_arrays(
                actual_arr,
                expected_arr,
                &mut CompareContext {
                    eps: params.eps,
                    path: params.path,
                    elem_path: params.elem_path,
                    diffs: params.diffs,
                    helper: compare_json_helper,
                },
            );
        }
        (JsonValue::Object(actual_obj), JsonValue::Object(expected_obj)) => {
            compare_objects(
                actual_obj,
                expected_obj,
                &mut CompareContext {
                    eps: params.eps,
                    path: params.path,
                    elem_path: params.elem_path,
                    diffs: params.diffs,
                    helper: compare_json_helper,
                },
            );
        }
        (actual_other, expected_other) => {
            compare_primitives(
                actual_value,
                expected_value,
                params.path,
                params.elem_path,
                params.diffs,
            );
            if !matches!(
                (actual_other, expected_other),
                (JsonValue::Bool(_), JsonValue::Bool(_))
                    | (JsonValue::String(_), JsonValue::String(_))
            ) {
                params.diffs.push(Difference {
                    path: format_path(params.path),
                    element_path: params.elem_path.join(" > "),
                    actual: type_name(actual_other).to_string(),
                    expected: type_name(expected_other).to_string(),
                });
            }
        }
    }
}

/// Filter out parent differences when a child has the same field difference.
/// For example, if both parent and child have rect.width differences, only keep the child's.
fn filter_redundant_diffs(diffs: &[Difference]) -> Vec<Difference> {
    let mut result = Vec::new();

    for (diff_index, diff) in diffs.iter().enumerate() {
        // Check if any later diff is for the same field but in a child element
        let is_redundant = diffs.iter().skip(diff_index + 1).any(|other_diff| {
            // Same field path (e.g., both are "rect.width")
            other_diff.path.ends_with(&diff.path)
                // Other diff is in a child element (longer element path)
                && other_diff.element_path.starts_with(&diff.element_path)
                && other_diff.element_path.len() > diff.element_path.len()
        });

        if !is_redundant {
            result.push(diff.clone());
        }
    }

    result
}

/// Compares two JSON values with epsilon tolerance for floating-point numbers.
///
/// # Errors
///
/// Returns an error string describing ALL mismatches if values differ beyond tolerance.
pub fn compare_json_with_epsilon(
    actual: &JsonValue,
    expected: &JsonValue,
    eps: f64,
) -> Result<(), String> {
    let mut elem_path: Vec<String> = Vec::new();
    if is_element_object(actual) && is_element_object(expected) {
        let tag = get_element_tag(actual)
            .or_else(|| get_element_tag(expected))
            .unwrap_or_else(|| "root".to_string());
        elem_path.push(tag);
    }

    let mut diffs = Vec::new();
    let mut path = Vec::new();
    compare_json_helper(
        actual,
        expected,
        &mut CompareParams {
            eps,
            path: &mut path,
            elem_path: &mut elem_path,
            diffs: &mut diffs,
        },
    );

    if diffs.is_empty() {
        return Ok(());
    }

    // Filter out redundant parent diffs
    let filtered_diffs = filter_redundant_diffs(&diffs);

    if filtered_diffs.is_empty() {
        return Ok(());
    }

    // Format all differences into a single error message
    let mut error_msg = format!("Found {} difference(s):\n\n", filtered_diffs.len());

    for diff in &filtered_diffs {
        let location = if diff.element_path.is_empty() {
            diff.path.clone()
        } else {
            format!("{} :: {}", diff.element_path, diff.path)
        };

        let _ignore_write_error = writeln!(
            error_msg,
            "  {} — actual: {}, expected: {}",
            location, diff.actual, diff.expected
        );
    }

    Err(error_msg)
}
