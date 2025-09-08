// Generic CSS value handling for simple single-token values.
// This is a minimal parser intended to evolve; it focuses on numeric lengths and idents.

#[derive(Debug, Clone, PartialEq)]
pub enum Unit {
    Px,
    Em,
    Rem,
    Percent,
    Other(String),
}

impl Unit {
    pub fn from_str(unit: &str) -> Self {
        match unit.to_ascii_lowercase().as_str() {
            "px" => Unit::Px,
            "em" => Unit::Em,
            "rem" => Unit::Rem,
            "%" => Unit::Percent, // not expected here; handled separately
            other => Unit::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Length {
    pub value: f32,
    pub unit: Unit,
}

impl Length {
    /// Convert this length to pixels using the provided context.
    /// - Px uses the raw value.
    /// - Em uses context.font_size_px.
    /// - Rem uses context.root_font_size_px.
    /// - Percent uses context.percent_base_px (value is in [0..100]).
    /// - Other returns None for now.
    pub fn to_px(&self, ctx: &LengthContext) -> Option<f32> {
        match self.unit {
            Unit::Px => Some(self.value),
            Unit::Em => Some(self.value * ctx.font_size_px),
            Unit::Rem => Some(self.value * ctx.root_font_size_px),
            Unit::Percent => Some(self.value * ctx.percent_base_px / 100.0),
            Unit::Other(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Length(Length),
    Number(f32),
    Ident(String),
}

/// Context for resolving relative CSS lengths to pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LengthContext {
    pub font_size_px: f32,
    pub root_font_size_px: f32,
    pub percent_base_px: f32,
}

impl Default for LengthContext {
    fn default() -> Self {
        Self {
            font_size_px: 16.0,
            root_font_size_px: 16.0,
            percent_base_px: 0.0,
        }
    }
}

/// Parse a CSS value from a raw string (single token expected).
/// Supports numeric numbers and lengths with units (px, em, rem, %, or other string units).
pub fn parse_value(input: &str) -> Option<Value> {
    let s = input.trim();
    if s.is_empty() { return None; }

    // Special-case percentage as a unit suffix
    if let Some(num) = s.strip_suffix('%') {
        if let Ok(n) = parse_number(num.trim()) { return Some(Value::Length(Length { value: n, unit: Unit::Percent })); }
    }

    // Extract leading sign+digits+dot from the start
    let (num_part, unit_part) = split_number_and_unit(s);
    if !num_part.is_empty() {
        if let Ok(n) = parse_number(num_part) {
            if unit_part.is_empty() {
                return Some(Value::Number(n));
            } else {
                return Some(Value::Length(Length { value: n, unit: Unit::from_str(unit_part) }));
            }
        }
    }

    // Fallback: treat as identifier/keyword
    Some(Value::Ident(s.to_string()))
}

/// Attempt to parse a length value (e.g., 12px, 1.5em, 50%).
pub fn parse_length(input: &str) -> Option<Length> {
    match parse_value(input)? {
        Value::Length(l) => Some(l),
        _ => None,
    }
}

/// Generic: convert a CSS length string to a rounded, non-negative pixel integer using a context.
/// Returns None if the value cannot be parsed or the unit is unsupported in this phase.
pub fn to_length_in_px_rounded_i32(input: &str, ctx: &LengthContext) -> Option<i32> {
    let len = parse_length(input)?;
    let px = len.to_px(ctx)?;
    Some(px.max(0.0).round() as i32)
}


fn split_number_and_unit(s: &str) -> (&str, &str) {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') { i += 1; }
    let mut has_digits = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; has_digits = true; }
    if i < bytes.len() && bytes[i] == b'.' { i += 1; }
    while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; has_digits = true; }
    if !has_digits { return ("", ""); }
    let (num, tail) = s.split_at(i);
    let unit = tail.trim();
    (num, unit)
}

fn parse_number(s: &str) -> Result<f32, std::num::ParseFloatError> {
    // Allow integers and floats; CSS allows numbers like ".5" but our split requires at least one digit
    s.parse::<f32>()
}
