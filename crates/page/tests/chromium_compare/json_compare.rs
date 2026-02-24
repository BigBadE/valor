use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use std::fmt::Write as _;

fn extract_id_label(value: &JsonValue) -> Option<String> {
    let JsonValue::Object(map) = value else {
        return None;
    };

    if let Some(JsonValue::Object(attrs)) = map.get("attrs") {
        if let Some(JsonValue::String(id)) = attrs.get("id") {
            if !id.is_empty() {
                return Some(format!("#{id}"));
            }
        }
    }

    if let Some(JsonValue::String(id)) = map.get("id") {
        if !id.is_empty() {
            return Some(format!("#{id}"));
        }
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

#[derive(Debug, Clone)]
struct Difference {
    path: String,
    element_path: String,
    actual: String,
    expected: String,
}

fn compare_numbers(
    actual: &JsonNumber,
    expected: &JsonNumber,
    eps: f64,
    path: &[String],
    elem_path: &[String],
    diffs: &mut Vec<Difference>,
) {
    match (actual.as_f64(), expected.as_f64()) {
        (Some(actual_float), Some(expected_float)) => {
            if (actual_float - expected_float).abs() > eps {
                diffs.push(Difference {
                    path: format_path(path),
                    element_path: elem_path.join(" > "),
                    actual: actual_float.to_string(),
                    expected: expected_float.to_string(),
                });
            }
        }
        _ => {
            diffs.push(Difference {
                path: format_path(path),
                element_path: elem_path.join(" > "),
                actual: actual.to_string(),
                expected: expected.to_string(),
            });
        }
    }
}

fn should_skip_root_rect_dimension(elem_path: &[String], path: &[String]) -> bool {
    elem_path.len() <= 1 && path.len() >= 2 && {
        let (last, prev) = (&path[path.len() - 1], &path[path.len() - 2]);
        matches!(last.as_str(), ".width" | ".height") && prev == ".rect"
    }
}

fn compare_json_recursive(
    actual: &JsonValue,
    expected: &JsonValue,
    eps: f64,
    path: &mut Vec<String>,
    elem_path: &mut Vec<String>,
    diffs: &mut Vec<Difference>,
) {
    if should_skip_root_rect_dimension(elem_path, path) {
        return;
    }

    match (actual, expected) {
        (JsonValue::Null, JsonValue::Null) => {}
        (JsonValue::Bool(actual_val), JsonValue::Bool(expected_val)) => {
            if actual_val != expected_val {
                diffs.push(Difference {
                    path: format_path(path),
                    element_path: elem_path.join(" > "),
                    actual: actual_val.to_string(),
                    expected: expected_val.to_string(),
                });
            }
        }
        (JsonValue::String(actual_str), JsonValue::String(expected_str)) => {
            // For text fields, normalize whitespace before comparing
            // (CSS white-space:normal collapses sequences into single spaces).
            let is_text_field = path.last().is_some_and(|seg| seg == ".text");
            let (a, e) = if is_text_field {
                (
                    actual_str.split_whitespace().collect::<Vec<_>>().join(" "),
                    expected_str
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            } else {
                (actual_str.clone(), expected_str.clone())
            };
            if a != e {
                diffs.push(Difference {
                    path: format_path(path),
                    element_path: elem_path.join(" > "),
                    actual: a,
                    expected: e,
                });
            }
        }
        (JsonValue::Number(actual_num), JsonValue::Number(expected_num)) => {
            compare_numbers(actual_num, expected_num, eps, path, elem_path, diffs);
        }
        (JsonValue::Array(actual_arr), JsonValue::Array(expected_arr)) => {
            compare_arrays(actual_arr, expected_arr, eps, path, elem_path, diffs);
        }
        (JsonValue::Object(actual_obj), JsonValue::Object(expected_obj)) => {
            compare_objects(actual_obj, expected_obj, eps, path, elem_path, diffs);
        }
        _ => {
            diffs.push(Difference {
                path: format_path(path),
                element_path: elem_path.join(" > "),
                actual: format!("{actual}"),
                expected: format!("{expected}"),
            });
        }
    }
}

fn compare_arrays(
    actual_arr: &[JsonValue],
    expected_arr: &[JsonValue],
    eps: f64,
    path: &mut Vec<String>,
    elem_path: &mut Vec<String>,
    diffs: &mut Vec<Difference>,
) {
    let is_asserts = path.last().is_some_and(|seg| seg == ".asserts");
    if is_asserts {
        return;
    }

    if actual_arr.len() != expected_arr.len() {
        diffs.push(Difference {
            path: format!("{}.length", format_path(path)),
            element_path: elem_path.join(" > "),
            actual: actual_arr.len().to_string(),
            expected: expected_arr.len().to_string(),
        });
        return;
    }

    let is_children = path.last().is_some_and(|seg| seg == ".children");

    for (idx, (actual_item, expected_item)) in
        actual_arr.iter().zip(expected_arr.iter()).enumerate()
    {
        if is_children {
            let label = extract_id_label(actual_item)
                .or_else(|| extract_id_label(expected_item))
                .unwrap_or_else(|| idx.to_string());
            path.pop();
            path.push(format!(".{label}"));
        } else {
            path.push(format!("[{idx}]"));
        }

        let push_elem = is_element_object(actual_item) && is_element_object(expected_item);
        if push_elem {
            let tag = get_element_tag(actual_item)
                .or_else(|| get_element_tag(expected_item))
                .unwrap_or_else(|| "?".to_string());
            let id = extract_id_label(actual_item)
                .or_else(|| extract_id_label(expected_item))
                .unwrap_or_else(|| format!("[{idx}]"));
            elem_path.push(format!("{tag}{id}"));
        }

        compare_json_recursive(actual_item, expected_item, eps, path, elem_path, diffs);

        if push_elem {
            elem_path.pop();
        }
        path.pop();
    }
}

fn compare_objects(
    actual_obj: &JsonMap<String, JsonValue>,
    expected_obj: &JsonMap<String, JsonValue>,
    eps: f64,
    path: &mut Vec<String>,
    elem_path: &mut Vec<String>,
    diffs: &mut Vec<Difference>,
) {
    let actual_count = actual_obj
        .keys()
        .filter(|key| key.as_str() != "fontFamily")
        .count();
    let expected_count = expected_obj
        .keys()
        .filter(|key| key.as_str() != "fontFamily")
        .count();

    if actual_count != expected_count {
        diffs.push(Difference {
            path: format!("{}.keys", format_path(path)),
            element_path: elem_path.join(" > "),
            actual: actual_count.to_string(),
            expected: expected_count.to_string(),
        });
    }

    for (key, actual_val) in actual_obj {
        if key == "fontFamily" {
            continue;
        }

        match expected_obj.get(key) {
            Some(expected_val) => {
                path.push(format!(".{key}"));

                let push_elem = is_element_object(actual_val) && is_element_object(expected_val);
                if push_elem {
                    let tag = get_element_tag(actual_val)
                        .or_else(|| get_element_tag(expected_val))
                        .unwrap_or_else(|| "?".to_string());
                    let id = extract_id_label(actual_val)
                        .or_else(|| extract_id_label(expected_val))
                        .unwrap_or_else(|| key.clone());
                    elem_path.push(format!("{tag}#{id}"));
                }

                compare_json_recursive(actual_val, expected_val, eps, path, elem_path, diffs);

                if push_elem {
                    elem_path.pop();
                }
                path.pop();
            }
            None => {
                diffs.push(Difference {
                    path: format_path(path),
                    element_path: elem_path.join(" > "),
                    actual: format!("has '{key}'"),
                    expected: "missing".to_string(),
                });
            }
        }
    }
}

fn filter_redundant_diffs(diffs: &[Difference]) -> Vec<Difference> {
    let mut result = Vec::new();
    for (idx, diff) in diffs.iter().enumerate() {
        let is_redundant = diffs.iter().skip(idx + 1).any(|other| {
            other.path.ends_with(&diff.path)
                && other.element_path.starts_with(&diff.element_path)
                && other.element_path.len() > diff.element_path.len()
        });
        if !is_redundant {
            result.push(diff.clone());
        }
    }
    result
}

/// Compare two JSON layout trees with epsilon tolerance.
///
/// Returns `Ok(())` if they match, or `Err(message)` with all differences.
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
    compare_json_recursive(actual, expected, eps, &mut path, &mut elem_path, &mut diffs);

    if diffs.is_empty() {
        return Ok(());
    }

    let filtered = filter_redundant_diffs(&diffs);
    if filtered.is_empty() {
        return Ok(());
    }

    let mut msg = format!("Found {} difference(s):\n\n", filtered.len());
    for diff in &filtered {
        let location = if diff.element_path.is_empty() {
            diff.path.clone()
        } else {
            format!("{} :: {}", diff.element_path, diff.path)
        };
        let _ = writeln!(
            msg,
            "  {location} — actual: {}, expected: {}",
            diff.actual, diff.expected
        );
    }

    Err(msg)
}
