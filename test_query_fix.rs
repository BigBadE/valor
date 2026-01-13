// Quick test to verify ComputedStyleQuery fix
use css_orchestrator::queries::{
    ComputedStyleQuery, DomChildrenInput, DomParentInput, DomTagInput,
};
use css_orchestrator::StyleDatabase;
use js::NodeKey;
use std::sync::Arc;
use valor_query::{InputQuery, Query, QueryDatabase};

fn main() {
    // Create a shared database
    let style_db = StyleDatabase::new();
    let shared_db = style_db.shared_query_db();

    // Create some test nodes
    let root = NodeKey::new(1);
    let child = NodeKey::new(2);

    // Set up DOM structure
    shared_db.set_input::<DomTagInput>(root, "div".to_string());
    shared_db.set_input::<DomTagInput>(child, "div".to_string());
    shared_db.set_input::<DomParentInput>(root, None);
    shared_db.set_input::<DomParentInput>(child, Some(root));
    shared_db.set_input::<DomChildrenInput>(root, vec![child]);
    shared_db.set_input::<DomChildrenInput>(child, vec![]);

    // Set up a simple stylesheet with width
    let css = r#"
        div {
            width: 300px;
            height: 100px;
        }
    "#;

    // Parse and apply stylesheet
    use css::CSSParser;
    let parser = CSSParser::new();
    let stylesheet = parser.parse_stylesheet(css);
    style_db.replace_stylesheet(stylesheet);

    // Query the computed style
    let style = shared_db.query::<ComputedStyleQuery>(child);

    println!("Child div computed style:");
    println!("  width: {:?}", style.width);
    println!("  height: {:?}", style.height);

    // Verify the width was parsed
    if let Some(width) = style.width {
        println!("\n✓ SUCCESS: Width parsed correctly as {}px", width);
        std::process::exit(0);
    } else {
        println!("\n✗ FAILURE: Width is None");
        std::process::exit(1);
    }
}
