//! Debug test to check CSS parsing.

use rewrite_css::matching::{StyleSheetsInput, parse_css};
use rewrite_page::Page;

#[test]
fn debug_css_parsing() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <style>
                .box {
                    width: 200px;
                }
            </style>
        </head>
        <body>
            <div class="box">Test</div>
        </body>
        </html>
    "#;

    let page = Page::from_html(html);

    // Check if stylesheets were parsed
    let stylesheets = page
        .database()
        .get_input::<StyleSheetsInput>(&())
        .unwrap_or_default();

    eprintln!("Number of rules: {}", stylesheets.rules.len());
    for (i, rule) in stylesheets.rules.iter().enumerate() {
        eprintln!(
            "Rule {}: selector='{}', specificity={}, declarations={:?}",
            i, rule.selector_text, rule.specificity, rule.declarations
        );
    }

    assert!(!stylesheets.rules.is_empty(), "No CSS rules were parsed");
}

#[test]
fn debug_direct_css_parse() {
    let css = ".box { width: 200px; }";
    let stylesheets = parse_css(css);

    eprintln!(
        "Direct parse - Number of rules: {}",
        stylesheets.rules.len()
    );
    for (i, rule) in stylesheets.rules.iter().enumerate() {
        eprintln!(
            "Rule {}: selector='{}', specificity={}, declarations={:?}",
            i, rule.selector_text, rule.specificity, rule.declarations
        );
    }

    assert!(
        !stylesheets.rules.is_empty(),
        "No CSS rules were parsed from direct CSS"
    );
}
