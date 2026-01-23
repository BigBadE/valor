//! Page handler that coordinates the HTML → CSS → Layout pipeline.

mod stylesheet;

use rewrite_core::{Database, DependencyContext, NodeId};
use rewrite_css::matching::{StyleSheetsInput, parse_css};
use rewrite_css::storage::CssPropertyInput;
use rewrite_css::{ColorValue, CssKeyword, CssValue};
use rewrite_html::{AttributeQuery, ChildrenQuery, TagNameQuery, TextContentQuery, parse_html};

pub use stylesheet::{CssRule, parse_stylesheet};

/// Page handler that manages the full pipeline.
pub struct Page {
    db: Database,
    root: NodeId,
}

impl Page {
    /// Create a new page from HTML.
    pub fn from_html(html: &str) -> Self {
        Self::from_html_with_viewport(html, None)
    }

    /// Create a new page from HTML with a specific viewport size.
    pub fn from_html_with_viewport(html: &str, viewport: Option<(f32, f32)>) -> Self {
        let (db, root) = parse_html(html);
        let mut page = Self { db, root };

        // Set viewport if provided
        if let Some((width, height)) = viewport {
            page.db.set_input::<rewrite_css::storage::ViewportInput>(
                (),
                rewrite_css::storage::ViewportSize { width, height },
            );
        }

        // Find the <html> element for rem unit resolution
        let mut ctx = DependencyContext::new();
        if let Some(html_element) = find_html_element(&page.db, root, &mut ctx) {
            page.db
                .set_input::<rewrite_css::storage::RootElementInput>((), html_element);
        }

        // Parse <style> tags and populate StyleSheets
        page.parse_style_tags(root);

        // Parse style attributes and set CSS inputs
        page.parse_style_attributes(root);

        page
    }

    /// Parse all <style> tags and populate the StyleSheets input.
    fn parse_style_tags(&mut self, node: NodeId) {
        let mut ctx = DependencyContext::new();
        let mut all_css = String::new();

        // Collect CSS from all <style> tags
        self.collect_style_tags(node, &mut all_css, &mut ctx);

        // Parse the combined CSS and set the StyleSheets input
        if !all_css.is_empty() {
            let stylesheets = parse_css(&all_css);
            self.db.set_input::<StyleSheetsInput>((), stylesheets);
        }
    }

    /// Recursively collect CSS text from all <style> tags.
    fn collect_style_tags(&self, node: NodeId, css_text: &mut String, ctx: &mut DependencyContext) {
        // Check if this is a <style> tag
        if let Some(tag_name) = self.db.query::<TagNameQuery>(node, ctx) {
            if tag_name == "style" {
                // Get all text content from children
                let children = self.db.query::<ChildrenQuery>(node, ctx);
                for &child in &children {
                    if let Some(text) = self.db.query::<TextContentQuery>(child, ctx) {
                        css_text.push_str(&text);
                        css_text.push('\n');
                    }
                }
            }
        }

        // Recursively process children
        let children = self.db.query::<ChildrenQuery>(node, ctx);
        for child in children {
            self.collect_style_tags(child, css_text, ctx);
        }
    }

    /// Get the database.
    pub fn database(&self) -> &Database {
        &self.db
    }

    /// Get the root node.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Parse style attributes recursively and set CSS inputs.
    fn parse_style_attributes(&mut self, node: NodeId) {
        let mut ctx = DependencyContext::new();

        // Apply user agent default styles based on element type
        self.apply_user_agent_styles(node, &mut ctx);

        // Check if this node has a style attribute
        if let Some(style) = self
            .db
            .query::<AttributeQuery>((node, "style".to_string()), &mut ctx)
        {
            self.parse_inline_style(node, &style);
        }

        // Process children recursively
        let children = self.db.query::<ChildrenQuery>(node, &mut ctx);
        for child in children {
            self.parse_style_attributes(child);
        }
    }

    /// Apply user agent default styles based on element tag name.
    fn apply_user_agent_styles(&mut self, node: NodeId, ctx: &mut DependencyContext) {
        use rewrite_css::{CssValue, LengthValue};
        use rewrite_html::TagNameQuery;

        // Get the element's tag name
        let Some(tag_name) = self.db.query::<TagNameQuery>(node, ctx) else {
            return;
        };

        // Apply default styles based on tag name
        match tag_name.as_str() {
            "head" => {
                // head { display: none }
                self.db.set_input::<CssPropertyInput>(
                    (node, "display".to_string()),
                    CssValue::Keyword(CssKeyword::None),
                );
                eprintln!("Set display:none for <head> node {:?}", node);
            }
            "body" => {
                // body { margin: 8px }
                let margin_8px = CssValue::Length(LengthValue::Px(8.0));
                for property in ["margin-top", "margin-right", "margin-bottom", "margin-left"] {
                    self.db.set_input::<CssPropertyInput>(
                        (node, property.to_string()),
                        margin_8px.clone(),
                    );
                }

                // DEBUG: Verify it was set
                let mut ctx = DependencyContext::new();
                let readback = self
                    .db
                    .query::<rewrite_css::storage::InheritedCssPropertyQuery>(
                        (node, "margin-top".to_string()),
                        &mut ctx,
                    );
                eprintln!(
                    "Set margin for <body> node {:?}: {:?}, readback: {:?}",
                    node, margin_8px, readback
                );
            }
            "p" => {
                // p { display: block; margin: 1em 0; }

                // Set display: block
                self.db.set_input::<CssPropertyInput>(
                    (node, "display".to_string()),
                    CssValue::Keyword(CssKeyword::Block),
                );

                // Set margins
                let margin_1em = CssValue::Length(LengthValue::Em(1.0));
                let margin_0 = CssValue::Length(LengthValue::Px(0.0));

                self.db.set_input::<CssPropertyInput>(
                    (node, "margin-top".to_string()),
                    margin_1em.clone(),
                );

                // DEBUG: Verify it was set
                let mut ctx = DependencyContext::new();
                let readback = self
                    .db
                    .query::<rewrite_css::storage::InheritedCssPropertyQuery>(
                        (node, "margin-top".to_string()),
                        &mut ctx,
                    );
                eprintln!(
                    "Set margin-top for <p> node {:?}: {:?}, readback: {:?}",
                    node, margin_1em, readback
                );
                self.db
                    .set_input::<CssPropertyInput>((node, "margin-bottom".to_string()), margin_1em);
                self.db.set_input::<CssPropertyInput>(
                    (node, "margin-left".to_string()),
                    margin_0.clone(),
                );
                self.db
                    .set_input::<CssPropertyInput>((node, "margin-right".to_string()), margin_0);
            }
            _ => {
                // No default styles for other elements yet
            }
        }
    }

    /// Parse inline style attribute and set CSS inputs.
    fn parse_inline_style(&mut self, node: NodeId, style: &str) {
        // Split by semicolon to get declarations
        for declaration in style.split(';') {
            let declaration = declaration.trim();
            if declaration.is_empty() {
                continue;
            }

            // Split by colon to get property and value
            if let Some((property, value)) = declaration.split_once(':') {
                let property = property.trim();
                let value = value.trim();

                // Check if this is a shorthand property
                match property {
                    "padding" => self.expand_padding_shorthand(node, value),
                    "margin" => self.expand_margin_shorthand(node, value),
                    "border-width" => self.expand_border_width_shorthand(node, value),
                    "gap" => self.expand_gap_shorthand(node, value),
                    _ => {
                        // Regular property - parse and set
                        if let Some(css_value) = parse_css_value(value) {
                            self.db.set_input::<CssPropertyInput>(
                                (node, property.to_string()),
                                css_value,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Expand padding shorthand (1-4 values).
    fn expand_padding_shorthand(&mut self, node: NodeId, value: &str) {
        let values = parse_shorthand_values(value);
        let (top, right, bottom, left) = expand_box_values(&values);

        if let Some(v) = top {
            self.db
                .set_input::<CssPropertyInput>((node, "padding-top".to_string()), v);
        }
        if let Some(v) = right {
            self.db
                .set_input::<CssPropertyInput>((node, "padding-right".to_string()), v);
        }
        if let Some(v) = bottom {
            self.db
                .set_input::<CssPropertyInput>((node, "padding-bottom".to_string()), v);
        }
        if let Some(v) = left {
            self.db
                .set_input::<CssPropertyInput>((node, "padding-left".to_string()), v);
        }
    }

    /// Expand margin shorthand (1-4 values).
    fn expand_margin_shorthand(&mut self, node: NodeId, value: &str) {
        let values = parse_shorthand_values(value);
        let (top, right, bottom, left) = expand_box_values(&values);

        if let Some(v) = top {
            self.db
                .set_input::<CssPropertyInput>((node, "margin-top".to_string()), v);
        }
        if let Some(v) = right {
            self.db
                .set_input::<CssPropertyInput>((node, "margin-right".to_string()), v);
        }
        if let Some(v) = bottom {
            self.db
                .set_input::<CssPropertyInput>((node, "margin-bottom".to_string()), v);
        }
        if let Some(v) = left {
            self.db
                .set_input::<CssPropertyInput>((node, "margin-left".to_string()), v);
        }
    }

    /// Expand border-width shorthand (1-4 values).
    fn expand_border_width_shorthand(&mut self, node: NodeId, value: &str) {
        let values = parse_shorthand_values(value);
        let (top, right, bottom, left) = expand_box_values(&values);

        if let Some(v) = top {
            self.db
                .set_input::<CssPropertyInput>((node, "border-top-width".to_string()), v);
        }
        if let Some(v) = right {
            self.db
                .set_input::<CssPropertyInput>((node, "border-right-width".to_string()), v);
        }
        if let Some(v) = bottom {
            self.db
                .set_input::<CssPropertyInput>((node, "border-bottom-width".to_string()), v);
        }
        if let Some(v) = left {
            self.db
                .set_input::<CssPropertyInput>((node, "border-left-width".to_string()), v);
        }
    }

    /// Expand gap shorthand (1-2 values).
    fn expand_gap_shorthand(&mut self, node: NodeId, value: &str) {
        let values = parse_shorthand_values(value);

        if values.is_empty() {
            return;
        }

        // gap: <row-gap> <column-gap>
        // If one value: both row and column get same value
        // If two values: first is row-gap, second is column-gap
        let row_gap = &values[0];
        let column_gap = values.get(1).unwrap_or(row_gap);

        self.db
            .set_input::<CssPropertyInput>((node, "row-gap".to_string()), row_gap.clone());
        self.db
            .set_input::<CssPropertyInput>((node, "column-gap".to_string()), column_gap.clone());
    }

    /// Compute layout for the page.
    pub fn compute_layout(&self) -> LayoutResult {
        // TODO: Implement layout computation
        // For now, just return dimensions for the root
        LayoutResult {
            root: self.root,
            // These would come from actual layout queries
            width: 0,
            height: 0,
        }
    }
}

/// Result of layout computation.
#[derive(Debug, Clone)]
pub struct LayoutResult {
    pub root: NodeId,
    pub width: i32,
    pub height: i32,
}

/// Parse shorthand values (space-separated).
fn parse_shorthand_values(value: &str) -> Vec<CssValue> {
    value
        .split_whitespace()
        .filter_map(parse_css_value)
        .collect()
}

/// Expand box shorthand values to (top, right, bottom, left).
/// Follows CSS convention:
/// - 1 value: all sides
/// - 2 values: vertical horizontal
/// - 3 values: top horizontal bottom
/// - 4 values: top right bottom left
fn expand_box_values(
    values: &[CssValue],
) -> (
    Option<CssValue>,
    Option<CssValue>,
    Option<CssValue>,
    Option<CssValue>,
) {
    match values.len() {
        0 => (None, None, None, None),
        1 => {
            let v = values[0].clone();
            (Some(v.clone()), Some(v.clone()), Some(v.clone()), Some(v))
        }
        2 => {
            let vertical = values[0].clone();
            let horizontal = values[1].clone();
            (
                Some(vertical.clone()),
                Some(horizontal.clone()),
                Some(vertical),
                Some(horizontal),
            )
        }
        3 => {
            let top = values[0].clone();
            let horizontal = values[1].clone();
            let bottom = values[2].clone();
            (
                Some(top),
                Some(horizontal.clone()),
                Some(bottom),
                Some(horizontal),
            )
        }
        _ => {
            // 4 or more values: top right bottom left
            (
                Some(values[0].clone()),
                Some(values[1].clone()),
                Some(values[2].clone()),
                Some(values[3].clone()),
            )
        }
    }
}

/// Parse a CSS value from a string.
fn parse_css_value(value: &str) -> Option<CssValue> {
    use rewrite_css::{CssKeyword, LengthValue};

    let value = value.trim();

    // Try parsing as keyword
    match value {
        "auto" => return Some(CssValue::Keyword(CssKeyword::Auto)),
        "none" => return Some(CssValue::Keyword(CssKeyword::None)),
        "block" => return Some(CssValue::Keyword(CssKeyword::Block)),
        "inline" => return Some(CssValue::Keyword(CssKeyword::Inline)),
        "flex" => return Some(CssValue::Keyword(CssKeyword::Flex)),
        _ => {}
    }

    // Try parsing as hex color
    if value.starts_with('#') {
        if let Some(color) = parse_hex_color(value) {
            return Some(CssValue::Color(color));
        }
    }

    // Try parsing as length
    if let Some(px_val) = value.strip_suffix("px") {
        if let Ok(num) = px_val.trim().parse::<f32>() {
            return Some(CssValue::Length(LengthValue::Px(num)));
        }
    }

    if let Some(em_val) = value.strip_suffix("em") {
        if let Ok(num) = em_val.trim().parse::<f32>() {
            return Some(CssValue::Length(LengthValue::Em(num)));
        }
    }

    if let Some(rem_val) = value.strip_suffix("rem") {
        if let Ok(num) = rem_val.trim().parse::<f32>() {
            return Some(CssValue::Length(LengthValue::Rem(num)));
        }
    }

    // Try parsing as percentage
    if let Some(pct_val) = value.strip_suffix('%') {
        if let Ok(num) = pct_val.trim().parse::<f32>() {
            return Some(CssValue::Percentage(num / 100.0));
        }
    }

    // Try parsing as plain number
    if let Ok(num) = value.parse::<f32>() {
        return Some(CssValue::Number(num));
    }

    None
}

/// Parse a hex color value (e.g., #ff0000, #f00)
fn parse_hex_color(hex: &str) -> Option<ColorValue> {
    let hex = hex.strip_prefix('#')?;

    let (r, g, b) = match hex.len() {
        // #rgb format
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            // Double each digit: #f00 -> #ff0000
            (r * 17, g * 17, b * 17)
        }
        // #rrggbb format
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            (r, g, b)
        }
        _ => return None,
    };

    Some(ColorValue { r, g, b, a: 255 })
}

/// Find the <html> element in the DOM tree.
fn find_html_element(db: &Database, node: NodeId, ctx: &mut DependencyContext) -> Option<NodeId> {
    use rewrite_html::TagNameQuery;

    // Check if this node is the <html> element
    if let Some(tag_name) = db.query::<TagNameQuery>(node, ctx) {
        if tag_name == "html" {
            return Some(node);
        }
    }

    // Recursively search children
    let children = db.query::<ChildrenQuery>(node, ctx);
    for &child in &children {
        if let Some(found) = find_html_element(db, child, ctx) {
            return Some(found);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_html() {
        let html = r#"
            <html>
                <body>
                    <div style="width: 100px; height: 50px">
                        <p>Hello World</p>
                    </div>
                </body>
            </html>
        "#;

        let page = Page::from_html(html);

        // Verify we have a database and root (root is node 0)
        assert_eq!(page.root.as_u64(), 0);
    }

    #[test]
    fn test_parse_css_value() {
        assert!(matches!(
            parse_css_value("100px"),
            Some(CssValue::Length(_))
        ));
        assert!(matches!(parse_css_value("2em"), Some(CssValue::Length(_))));
        assert!(matches!(
            parse_css_value("50%"),
            Some(CssValue::Percentage(_))
        ));
        assert!(matches!(
            parse_css_value("auto"),
            Some(CssValue::Keyword(_))
        ));
        assert!(matches!(parse_css_value("1.5"), Some(CssValue::Number(_))));
    }
}
