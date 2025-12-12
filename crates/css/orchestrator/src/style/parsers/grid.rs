//! Grid property parsers.

use std::collections::HashMap;

use crate::style_model;

/// Parse and apply grid properties.
///
/// For MVP, we store the raw string values for grid-template-columns/rows
/// and parse them during layout.
pub fn apply_grid_properties(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    if let Some(value) = decls.get("grid-template-columns") {
        computed.grid_template_columns = Some(value.clone());
    }
    if let Some(value) = decls.get("grid-template-rows") {
        computed.grid_template_rows = Some(value.clone());
    }
    if let Some(value) = decls.get("grid-auto-flow") {
        computed.grid_auto_flow = if value.eq_ignore_ascii_case("column") {
            style_model::GridAutoFlow::Column
        } else if value.eq_ignore_ascii_case("row dense") || value.eq_ignore_ascii_case("dense") {
            style_model::GridAutoFlow::RowDense
        } else if value.eq_ignore_ascii_case("column dense") {
            style_model::GridAutoFlow::ColumnDense
        } else {
            style_model::GridAutoFlow::Row
        };
    }
}
