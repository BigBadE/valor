use serde_json::{Number as JsonNumber, Value as JsonValue, value::Map as JsonMap};

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

fn extract_rect_coords(rect_map: &JsonMap<String, JsonValue>) -> (f64, f64, f64, f64) {
    let coord_x = rect_map.get("x").and_then(JsonValue::as_f64).unwrap_or(0.0);
    let coord_y = rect_map.get("y").and_then(JsonValue::as_f64).unwrap_or(0.0);
    let coord_width = rect_map
        .get("width")
        .and_then(JsonValue::as_f64)
        .unwrap_or(0.0);
    let coord_height = rect_map
        .get("height")
        .and_then(JsonValue::as_f64)
        .unwrap_or(0.0);
    (coord_x, coord_y, coord_width, coord_height)
}

fn format_child_summary(child_tag: &str, child_id: &str, rect: (f64, f64, f64, f64)) -> String {
    let (coord_x, coord_y, coord_width, coord_height) = rect;
    if child_id.is_empty() {
        format!("<{child_tag}> rect=({coord_x:.0},{coord_y:.0},{coord_width:.0},{coord_height:.0})")
    } else {
        format!(
            "<{child_tag} id=#{child_id}> rect=({coord_x:.0},{coord_y:.0},{coord_width:.0},{coord_height:.0})"
        )
    }
}

fn process_child_element(child: &JsonValue) -> Option<String> {
    if let JsonValue::Object(child_map) = child {
        let child_tag = child_map
            .get("tag")
            .and_then(|tag_val| tag_val.as_str())
            .unwrap_or("");
        let child_id = child_map
            .get("id")
            .and_then(|id_val| id_val.as_str())
            .unwrap_or("");
        let rect = if let Some(JsonValue::Object(rect_obj)) = child_map.get("rect") {
            extract_rect_coords(rect_obj)
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };
        Some(format_child_summary(child_tag, child_id, rect))
    } else {
        None
    }
}

fn child_summary_lines(children: &[JsonValue]) -> Vec<JsonValue> {
    let mut lines: Vec<JsonValue> = Vec::new();
    for child in children {
        if let Some(summary) = process_child_element(child) {
            lines.push(JsonValue::String(summary));
        }
    }
    lines
}

fn pretty_elem_with_compact_children(value: &JsonValue) -> String {
    use serde_json::to_string_pretty;

    if let JsonValue::Object(map) = value {
        let mut output_map = map.clone();
        if let Some(JsonValue::Array(children)) = map.get("children") {
            let lines = child_summary_lines(children);
            output_map.insert("children".to_string(), JsonValue::Array(lines));
        }
        let json_object = JsonValue::Object(output_map);
        to_string_pretty(&json_object).unwrap_or_else(|_| String::from("{}"))
    } else {
        to_string_pretty(value).unwrap_or_else(|_| String::from("{}"))
    }
}

fn build_err(
    kind: &str,
    detail: &str,
    path: &[String],
    elem_stack: &[(JsonValue, JsonValue)],
) -> String {
    let path_str = format_path(path);
    let (our_elem, chromium_elem) = if let Some((valor_elem, chromium_elem)) = elem_stack.last() {
        (valor_elem, chromium_elem)
    } else {
        (&JsonValue::Null, &JsonValue::Null)
    };
    let our_str = pretty_elem_with_compact_children(our_elem);
    let chromium_str = pretty_elem_with_compact_children(chromium_elem);
    if detail.is_empty() {
        format!("{path_str}: {kind}\nElement (our): {our_str}\nElement (chromium): {chromium_str}")
    } else {
        format!(
            "{path_str}: {kind} — {detail}\nElement (our): {our_str}\nElement (chromium): {chromium_str}"
        )
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

type HelperFn = fn(
    &JsonValue,
    &JsonValue,
    f64,
    &mut Vec<String>,
    &mut Vec<(JsonValue, JsonValue)>,
) -> Result<(), String>;

struct CompareContext<'cmp> {
    eps: f64,
    path: &'cmp mut Vec<String>,
    elem_stack: &'cmp mut Vec<(JsonValue, JsonValue)>,
    helper: HelperFn,
}

/// Compares two JSON numbers with an epsilon tolerance.
///
/// # Errors
///
/// Returns an error if the numbers differ by more than the epsilon or are non-float numbers.
fn compare_numbers(
    actual: &JsonNumber,
    expected: &JsonNumber,
    eps: f64,
    path: &[String],
    elem_stack: &[(JsonValue, JsonValue)],
) -> Result<(), String> {
    match (actual.as_f64(), expected.as_f64()) {
        (Some(actual_float), Some(expected_float)) => {
            if (actual_float - expected_float).abs() <= eps {
                Ok(())
            } else {
                Err(build_err(
                    "number diff",
                    &format!("{actual_float} vs {expected_float} exceeds eps {eps}"),
                    path,
                    elem_stack,
                ))
            }
        }
        _ => Err(build_err(
            "non-float number encountered",
            "",
            path,
            elem_stack,
        )),
    }
}

/// Compares two JSON arrays element by element.
///
/// # Errors
///
/// Returns an error if array lengths differ or any elements differ.
fn compare_arrays(
    actual_arr: &[JsonValue],
    expected_arr: &[JsonValue],
    ctx: &mut CompareContext<'_>,
) -> Result<(), String> {
    if actual_arr.len() != expected_arr.len() {
        return Err(build_err(
            "array length mismatch",
            &format!("{} != {}", actual_arr.len(), expected_arr.len()),
            ctx.path,
            ctx.elem_stack,
        ));
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
            ctx.elem_stack
                .push((actual_item.clone(), expected_item.clone()));
        }
        let result = (ctx.helper)(
            actual_item,
            expected_item,
            ctx.eps,
            ctx.path,
            ctx.elem_stack,
        );
        if should_push {
            ctx.elem_stack.pop();
        }
        ctx.path.pop();
        result?;
    }
    Ok(())
}

/// Compares two JSON objects key by key.
///
/// # Errors
///
/// Returns an error if object sizes differ, keys are missing, or any values differ.
fn compare_objects(
    actual_obj: &JsonMap<String, JsonValue>,
    expected_obj: &JsonMap<String, JsonValue>,
    ctx: &mut CompareContext<'_>,
) -> Result<(), String> {
    if actual_obj.len() != expected_obj.len() {
        return Err(build_err(
            "object size mismatch",
            &format!("{} != {}", actual_obj.len(), expected_obj.len()),
            ctx.path,
            ctx.elem_stack,
        ));
    }
    for (key, actual_val) in actual_obj {
        match expected_obj.get(key) {
            Some(expected_val) => {
                ctx.path.push(format!(".{key}"));
                let should_push = is_element_object(actual_val) && is_element_object(expected_val);
                if should_push {
                    ctx.elem_stack
                        .push((actual_val.clone(), expected_val.clone()));
                }
                let result =
                    (ctx.helper)(actual_val, expected_val, ctx.eps, ctx.path, ctx.elem_stack);
                if should_push {
                    ctx.elem_stack.pop();
                }
                ctx.path.pop();
                result?;
            }
            None => {
                return Err(build_err(
                    "missing key in expected",
                    &format!("'{key}'"),
                    ctx.path,
                    ctx.elem_stack,
                ));
            }
        }
    }
    Ok(())
}

/// Helper function to recursively compare JSON values.
///
/// # Errors
///
/// Returns an error string if values differ beyond tolerance.
fn compare_json_helper(
    actual_value: &JsonValue,
    expected_value: &JsonValue,
    eps: f64,
    path: &mut Vec<String>,
    elem_stack: &mut Vec<(JsonValue, JsonValue)>,
) -> Result<(), String> {
    // Special-case: ignore root-level rect width/height diffs
    if elem_stack.len() <= 1 && path.len() >= 2 {
        let (last, prev) = (&path[path.len() - 1], &path[path.len() - 2]);
        if matches!(last.as_str(), ".width" | ".height") && prev == ".rect" {
            return Ok(());
        }
    }
    match (actual_value, expected_value) {
        (JsonValue::Null, JsonValue::Null) => Ok(()),
        (JsonValue::Bool(actual_bool), JsonValue::Bool(expected_bool))
            if actual_bool == expected_bool =>
        {
            Ok(())
        }
        (JsonValue::Bool(actual_bool), JsonValue::Bool(expected_bool)) => Err(build_err(
            "bool mismatch",
            &format!("{actual_bool} != {expected_bool}"),
            path,
            elem_stack,
        )),
        (JsonValue::Number(actual_num), JsonValue::Number(expected_num)) => {
            compare_numbers(actual_num, expected_num, eps, path, elem_stack)
        }
        (JsonValue::String(actual_str), JsonValue::String(expected_str))
            if actual_str == expected_str =>
        {
            Ok(())
        }
        (JsonValue::String(actual_str), JsonValue::String(expected_str)) => Err(build_err(
            "string mismatch",
            &format!("'{actual_str}' != '{expected_str}'"),
            path,
            elem_stack,
        )),
        (JsonValue::Array(actual_arr), JsonValue::Array(expected_arr)) => compare_arrays(
            actual_arr,
            expected_arr,
            &mut CompareContext {
                eps,
                path,
                elem_stack,
                helper: compare_json_helper,
            },
        ),
        (JsonValue::Object(actual_obj), JsonValue::Object(expected_obj)) => compare_objects(
            actual_obj,
            expected_obj,
            &mut CompareContext {
                eps,
                path,
                elem_stack,
                helper: compare_json_helper,
            },
        ),
        (actual_other, expected_other) => Err(build_err(
            "type mismatch",
            &format!(
                "{:?} vs {:?}",
                type_name(actual_other),
                type_name(expected_other)
            ),
            path,
            elem_stack,
        )),
    }
}

/// Compares two JSON values with epsilon tolerance for floating-point numbers.
///
/// # Errors
///
/// Returns an error string describing the mismatch if values differ beyond tolerance.
pub fn compare_json_with_epsilon(
    actual: &JsonValue,
    expected: &JsonValue,
    eps: f64,
) -> Result<(), String> {
    let mut elem_stack: Vec<(JsonValue, JsonValue)> = Vec::new();
    if is_element_object(actual) && is_element_object(expected) {
        elem_stack.push((actual.clone(), expected.clone()));
    }
    compare_json_helper(actual, expected, eps, &mut Vec::new(), &mut elem_stack)
}
