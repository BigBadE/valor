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
        "table",
        "thead",
        "tbody",
        "tfoot",
        "tr",
        "th",
        "td",
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
        "select",
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

/// Create form control user-agent rules per HTML5 spec and browser defaults.
fn create_form_control_rules(mut source_order: u32) -> (Vec<types::Rule>, u32) {
    let mut rules = Vec::new();

    // button { display: block; padding: 6px 10px; border: 1px solid; box-sizing: border-box; min-height: 20px }
    // Note: min-height ensures buttons have reasonable height even without explicit content
    rules.push(make_ua_rule(
        "button",
        source_order,
        &[
            ("display", "block"),
            ("padding", "6px 10px"),
            ("border", "1px solid"),
            ("box-sizing", "border-box"),
            ("min-height", "20px"),
        ],
    ));
    source_order += 1;

    // input { display: inline-block; padding: 8px 12px; border: 2px solid; box-sizing: border-box; overflow: clip }
    rules.push(make_ua_rule(
        "input",
        source_order,
        &[
            ("display", "inline-block"),
            ("padding", "8px 12px"),
            ("border", "2px solid"),
            ("box-sizing", "border-box"),
            ("overflow", "clip"),
        ],
    ));
    source_order += 1;

    // input[type="checkbox"] { display: inline-block; padding: 0; border: 0; overflow: visible }
    rules.push(make_ua_rule(
        "input[type=\"checkbox\"]",
        source_order,
        &[
            ("display", "inline-block"),
            ("padding", "0"),
            ("border", "0"),
            ("font-size", "13.3333px"),
            ("overflow", "visible"),
        ],
    ));
    source_order += 1;

    // input[type="radio"] { display: inline-block; padding: 0; border: 0; overflow: visible }
    rules.push(make_ua_rule(
        "input[type=\"radio\"]",
        source_order,
        &[
            ("display", "inline-block"),
            ("padding", "0"),
            ("border", "0"),
            ("font-size", "13.3333px"),
            ("overflow", "visible"),
        ],
    ));
    source_order += 1;

    // label { display: inline }
    rules.push(make_ua_rule(
        "label",
        source_order,
        &[("display", "inline")],
    ));
    source_order += 1;

    // textarea { display: inline-block; padding: 10px; border: 2px solid; box-sizing: border-box; overflow: auto }
    rules.push(make_ua_rule(
        "textarea",
        source_order,
        &[
            ("display", "inline-block"),
            ("padding", "10px"),
            ("border", "2px solid"),
            ("box-sizing", "border-box"),
            ("overflow", "auto"),
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
    source_order = next_order;

    let (form_rules, next_order_after_forms) = create_form_control_rules(source_order);
    rules.extend(form_rules);
    source_order = next_order_after_forms;

    // Add heading styles to match browser defaults
    // Font sizes based on Chrome/Firefox user-agent stylesheets
    rules.push(make_ua_rule(
        "h1",
        source_order,
        &[("font-weight", "700"), ("font-size", "2em")],
    ));
    source_order += 1;
    rules.push(make_ua_rule(
        "h2",
        source_order,
        &[("font-weight", "700"), ("font-size", "1.5em")],
    ));
    source_order += 1;
    rules.push(make_ua_rule(
        "h3",
        source_order,
        &[("font-weight", "700"), ("font-size", "1.17em")],
    ));
    source_order += 1;
    rules.push(make_ua_rule(
        "h4",
        source_order,
        &[("font-weight", "700"), ("font-size", "1em")],
    ));
    source_order += 1;
    rules.push(make_ua_rule(
        "h5",
        source_order,
        &[("font-weight", "700"), ("font-size", "0.83em")],
    ));
    source_order += 1;
    rules.push(make_ua_rule(
        "h6",
        source_order,
        &[("font-weight", "700"), ("font-size", "0.67em")],
    ));

    types::Stylesheet {
        rules,
        origin: types::Origin::UserAgent,
    }
}
