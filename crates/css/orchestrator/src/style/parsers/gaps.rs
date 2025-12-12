//! Gap property parsers (row-gap, column-gap, gap).

use std::collections::HashMap;

use crate::style_model;

use super::super::parse_px;

/// Represents a parsed gap token that may be either pixels or a percentage fraction (0.0..=1.0).
enum EitherGap {
    /// Pixel value variant.
    Pixels(f32),
    /// Percentage value expressed as a 0.0..=1.0 fraction.
    PercentValue(f32),
}

/// Parse a gap token into either px or percentage (0.0..=1.0).
fn parse_gap_token(token_text: &str) -> Option<EitherGap> {
    if let Some(px_value) = parse_px(token_text) {
        return Some(EitherGap::Pixels(px_value));
    }
    let trimmed = token_text.trim();
    if let Some(percent_str) = trimmed.strip_suffix('%')
        && let Ok(percent_value) = percent_str.trim().parse::<f32>()
    {
        return Some(EitherGap::PercentValue((percent_value / 100.0).max(0.0)));
    }
    None
}

/// Apply a `gap:` shorthand with one or two tokens, updating px and percent fields accordingly.
fn apply_pair_gap(
    value: &str,
    row_gap: &mut f32,
    col_gap: &mut f32,
    row_percent: &mut Option<f32>,
    col_percent: &mut Option<f32>,
) {
    let parts: Vec<&str> = value
        .split(|character: char| character.is_ascii_whitespace())
        .filter(|segment| !segment.is_empty())
        .collect();
    if parts.len() == 1 {
        if let Some(first_token) = parts.first()
            && let Some(parsed) = parse_gap_token(first_token)
        {
            match parsed {
                EitherGap::Pixels(px_value) => {
                    *row_gap = px_value;
                    *col_gap = px_value;
                    *row_percent = None;
                    *col_percent = None;
                }
                EitherGap::PercentValue(percent_fraction) => {
                    *row_percent = Some(percent_fraction);
                    *col_percent = Some(percent_fraction);
                    *row_gap = 0.0;
                    *col_gap = 0.0;
                }
            }
        }
    } else if parts.len() >= 2 {
        if let Some(first) = parts.first()
            && let Some(parsed) = parse_gap_token(first)
        {
            match parsed {
                EitherGap::Pixels(px_value) => {
                    *row_gap = px_value;
                    *row_percent = None;
                }
                EitherGap::PercentValue(percent_fraction) => {
                    *row_percent = Some(percent_fraction);
                    *row_gap = 0.0;
                }
            }
        }
        if let Some(second) = parts.get(1)
            && let Some(parsed) = parse_gap_token(second)
        {
            match parsed {
                EitherGap::Pixels(px_value) => {
                    *col_gap = px_value;
                    *col_percent = None;
                }
                EitherGap::PercentValue(percent_fraction) => {
                    *col_percent = Some(percent_fraction);
                    *col_gap = 0.0;
                }
            }
        }
    }
}

/// Apply a single longhand gap token (row-gap or column-gap) to px or percent fields.
fn apply_single_gap(token_text: &str, gap_px: &mut f32, gap_percent: &mut Option<f32>) {
    if let Some(parsed) = parse_gap_token(token_text) {
        match parsed {
            EitherGap::Pixels(px_value) => {
                *gap_px = px_value;
                *gap_percent = None;
            }
            EitherGap::PercentValue(percent_fraction) => {
                *gap_percent = Some(percent_fraction);
                *gap_px = 0.0;
            }
        }
    }
}

/// Parse `gap`, `row-gap`, and `column-gap` (px or %). Percentages are stored and resolved later.
pub fn apply_gaps(computed: &mut style_model::ComputedStyle, decls: &HashMap<String, String>) {
    // Defaults per spec are 0 when not specified. Percent fields default to None.
    let mut row_gap_px = computed.row_gap;
    let mut col_gap_px = computed.column_gap;
    let mut row_gap_percent = computed.row_gap_percent;
    let mut col_gap_percent = computed.column_gap_percent;

    if let Some(gap_value) = decls.get("gap") {
        apply_pair_gap(
            gap_value,
            &mut row_gap_px,
            &mut col_gap_px,
            &mut row_gap_percent,
            &mut col_gap_percent,
        );
    }

    if let Some(row_value) = decls.get("row-gap") {
        apply_single_gap(row_value, &mut row_gap_px, &mut row_gap_percent);
    }
    if let Some(col_value) = decls.get("column-gap") {
        apply_single_gap(col_value, &mut col_gap_px, &mut col_gap_percent);
    }

    computed.row_gap = row_gap_px.max(0.0);
    computed.column_gap = col_gap_px.max(0.0);
    computed.row_gap_percent = row_gap_percent;
    computed.column_gap_percent = col_gap_percent;
}
