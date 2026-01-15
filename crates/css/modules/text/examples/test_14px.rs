use css_orchestrator::style_model::ComputedStyle;
use css_text::measurement::measure_text;

fn main() {
    // Test with 14px sans-serif (system-ui font)
    let mut style = ComputedStyle::default();
    style.font_size = 14.0;
    style.font_family = Some("system-ui".to_string());
    style.line_height = None; // Should use "normal"

    let metrics = measure_text("Primary", &style);

    println!("Font size: 14px");
    println!("Line height (normal): {}", metrics.height);
    println!("Glyph height: {}", metrics.glyph_height);
    println!("Ascent: {}", metrics.ascent);
    println!("Descent: {}", metrics.descent);
    println!("");
    println!("Expected content height for button: 21px (37 - 16 padding)");
    println!("Actual line-height: {}", metrics.height);
}
