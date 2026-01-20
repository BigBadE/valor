//! User-agent stylesheet implementation.
//!
//! Provides default styling rules for HTML elements according to browser defaults
//! and the HTML5 specification.

use crate::types;

/// List of block-level HTML elements from the HTML5 spec.
fn block_level_elements() -> Vec<&'static str> {
    vec![
        "html",
        "body",
        "div",
        "p",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "ul",
        "ol",
        "li",
        "dl",
        "dt",
        "dd",
        "blockquote",
        "pre",
        "form",
        "fieldset",
        "legend",
        "section",
        "article",
        "aside",
        "header",
        "footer",
        "main",
        "nav",
        "address",
        "figure",
        "figcaption",
        "details",
        "summary",
        "dialog",
        "hr",
        "button",
        "textarea",
    ]
}

/// Create rules for block-level display elements.
fn create_block_display_rules(source_order_start: u32) -> (Vec<types::Rule>, u32) {
    let block_elements = block_level_elements();
    let mut rules = Vec::with_capacity(block_elements.len());
    let mut source_order = source_order_start;

    for tag in block_elements {
        rules.push(types::Rule {
            origin: types::Origin::UserAgent,
            source_order,
            prelude: tag.to_string(),
            declarations: vec![types::Declaration {
                name: "display".to_string(),
                value: "block".to_string(),
                important: false,
            }],
        });
        source_order += 1;
    }

    (rules, source_order)
}

/// Helper to create a UA rule with given selector and declarations.
fn make_ua_rule(selector: &str, order: u32, props: &[(&str, &str)]) -> types::Rule {
    types::Rule {
        origin: types::Origin::UserAgent,
        source_order: order,
        prelude: selector.to_string(),
        declarations: props
            .iter()
            .map(|(name, value)| types::Declaration {
                name: (*name).to_string(),
                value: (*value).to_string(),
                important: false,
            })
            .collect(),
    }
}

/// Create button rule.
fn create_button_rule(source_order: u32) -> types::Rule {
    // Note: Form controls don't inherit font-family per HTML spec - they use a system default
    make_ua_rule(
        "button",
        source_order,
        &[
            ("display", "block"),
            ("padding", "0"),
            ("border", "1px solid"),
            ("box-sizing", "border-box"),
            ("font-family", "Arial"),
        ],
    )
}

/// Create input rules (base, checkbox, radio).
fn create_input_rules(mut source_order: u32) -> (Vec<types::Rule>, u32) {
    let mut rules = Vec::new();

    rules.push(make_ua_rule(
        "input",
        source_order,
        &[
            ("display", "inline-block"),
            ("padding", "0"),
            ("border", "2px solid"),
            ("box-sizing", "border-box"),
            ("overflow", "clip"),
            ("font-family", "Arial"),
            ("width", "173px"), // Default width for text inputs (matches Chrome)
            ("height", "19px"), // Default height for text inputs (matches Chrome)
        ],
    ));
    source_order += 1;

    rules.push(make_ua_rule(
        "input[type=\"checkbox\"]",
        source_order,
        &[
            ("display", "inline-block"),
            ("padding", "0"),
            ("border", "0"),
            ("font-size", "13.3333px"),
            ("overflow", "visible"),
            ("width", "16px"), // Explicit size for checkboxes
            ("height", "16px"),
        ],
    ));
    source_order += 1;

    rules.push(make_ua_rule(
        "input[type=\"radio\"]",
        source_order,
        &[
            ("display", "inline-block"),
            ("padding", "0"),
            ("border", "0"),
            ("font-size", "13.3333px"),
            ("overflow", "visible"),
            ("width", "16px"), // Explicit size for radio buttons
            ("height", "16px"),
        ],
    ));
    source_order += 1;

    (rules, source_order)
}

/// Create table display rules per HTML5 and CSS spec.
fn create_table_display_rules(mut source_order: u32) -> (Vec<types::Rule>, u32) {
    let mut rules = Vec::new();

    rules.push(make_ua_rule("table", source_order, &[("display", "table")]));
    source_order += 1;

    rules.push(make_ua_rule(
        "thead",
        source_order,
        &[("display", "table-header-group")],
    ));
    source_order += 1;

    rules.push(make_ua_rule(
        "tbody",
        source_order,
        &[("display", "table-row-group")],
    ));
    source_order += 1;

    rules.push(make_ua_rule(
        "tfoot",
        source_order,
        &[("display", "table-footer-group")],
    ));
    source_order += 1;

    rules.push(make_ua_rule(
        "tr",
        source_order,
        &[("display", "table-row")],
    ));
    source_order += 1;

    rules.push(make_ua_rule(
        "th",
        source_order,
        &[
            ("display", "table-cell"),
            ("padding", "12px 16px"),
            ("font-weight", "600"),
        ],
    ));
    source_order += 1;

    rules.push(make_ua_rule(
        "td",
        source_order,
        &[("display", "table-cell"), ("padding", "12px 16px")],
    ));
    source_order += 1;

    (rules, source_order)
}

/// Create form control user-agent rules per HTML5 spec and browser defaults.
fn create_form_control_rules(mut source_order: u32) -> (Vec<types::Rule>, u32) {
    let mut rules = Vec::new();

    rules.push(create_button_rule(source_order));
    source_order += 1;

    let (input_rules, next_order) = create_input_rules(source_order);
    rules.extend(input_rules);
    source_order = next_order;

    rules.push(make_ua_rule(
        "label",
        source_order,
        &[("display", "inline")],
    ));
    source_order += 1;

    rules.push(make_ua_rule(
        "textarea",
        source_order,
        &[
            ("display", "inline-block"),
            ("padding", "0"),
            ("border", "2px solid"),
            ("box-sizing", "border-box"),
            ("overflow", "auto"),
            ("font-family", "Arial"),
            ("width", "200px"),  // Default width
            ("height", "100px"), // Default height for textarea
        ],
    ));
    source_order += 1;

    rules.push(make_ua_rule(
        "select",
        source_order,
        &[
            ("display", "inline-block"),
            ("align-items", "center"),
            ("font-family", "Arial"),
            ("padding", "0"),
            ("border", "1px solid"),
            ("width", "200px"), // Default width for select
        ],
    ));
    source_order += 1;

    (rules, source_order)
}

/// Create a minimal user-agent stylesheet with default display values for block-level HTML elements.
pub fn create_ua_stylesheet() -> types::Stylesheet {
    let mut rules = Vec::new();
    let mut source_order = 0u32;

    // Add default color rule for the root element
    rules.push(make_ua_rule("html", source_order, &[("color", "#000")]));
    source_order += 1;

    // Hide HTML metadata elements per HTML5 spec
    // These elements should not be rendered and should not participate in layout
    let hidden_elements = [
        "head", "meta", "title", "link", "style", "script", "base", "template", "noscript",
    ];

    for tag in &hidden_elements {
        rules.push(make_ua_rule(tag, source_order, &[("display", "none")]));
        source_order += 1;
    }

    let (block_rules, next_order) = create_block_display_rules(source_order);
    rules.extend(block_rules);

    let (table_rules, next_order) = create_table_display_rules(next_order);
    rules.extend(table_rules);

    let (form_rules, next_order) = create_form_control_rules(next_order);
    rules.extend(form_rules);
    source_order = next_order;

    // Add heading styles to match browser defaults
    // Font sizes and margins based on Chrome/Firefox user-agent stylesheets
    // Margins are 0.67em top/bottom for h1, 0.83em for h2/h3, 1.33em for h4, etc.
    // NOTE: Margins are specified as explicit longhands (margin-top, margin-bottom, etc.)
    // instead of shorthands to ensure proper cascade behavior when user stylesheets
    // override specific sides (e.g., "margin-top: 0" should override only top margin).
    rules.push(make_ua_rule(
        "h1",
        source_order,
        &[
            ("font-weight", "700"),
            ("font-size", "2em"),
            ("margin-top", "0.67em"),
            ("margin-right", "0"),
            ("margin-bottom", "0.67em"),
            ("margin-left", "0"),
        ],
    ));
    source_order += 1;
    rules.push(make_ua_rule(
        "h2",
        source_order,
        &[
            ("font-weight", "700"),
            ("font-size", "1.5em"),
            ("margin-top", "0.83em"),
            ("margin-right", "0"),
            ("margin-bottom", "0.83em"),
            ("margin-left", "0"),
        ],
    ));
    source_order += 1;
    rules.push(make_ua_rule(
        "h3",
        source_order,
        &[
            ("font-weight", "700"),
            ("font-size", "1.17em"),
            ("margin-top", "1em"),
            ("margin-right", "0"),
            ("margin-bottom", "1em"),
            ("margin-left", "0"),
        ],
    ));
    source_order += 1;
    rules.push(make_ua_rule(
        "h4",
        source_order,
        &[
            ("font-weight", "700"),
            ("font-size", "1em"),
            ("margin-top", "1.33em"),
            ("margin-right", "0"),
            ("margin-bottom", "1.33em"),
            ("margin-left", "0"),
        ],
    ));
    source_order += 1;
    rules.push(make_ua_rule(
        "h5",
        source_order,
        &[
            ("font-weight", "700"),
            ("font-size", "0.83em"),
            ("margin-top", "1.67em"),
            ("margin-right", "0"),
            ("margin-bottom", "1.67em"),
            ("margin-left", "0"),
        ],
    ));
    source_order += 1;
    rules.push(make_ua_rule(
        "h6",
        source_order,
        &[
            ("font-weight", "700"),
            ("font-size", "0.67em"),
            ("margin-top", "2.33em"),
            ("margin-right", "0"),
            ("margin-bottom", "2.33em"),
            ("margin-left", "0"),
        ],
    ));

    types::Stylesheet {
        rules,
        origin: types::Origin::UserAgent,
    }
}
