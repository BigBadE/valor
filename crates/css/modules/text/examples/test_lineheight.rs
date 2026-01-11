use css_orchestrator::style_model::ComputedStyle;
use css_text::measurement::measure_text;

fn main() {
    // Test with 24px Times New Roman (Liberation Serif)
    let mut style = ComputedStyle::default();
    style.font_size = 24.0;
    style.font_family = Some("serif".to_string());
    style.line_height = None; // Should use "normal"

    let metrics = measure_text("Hello World", &style);

    println!("Font size: 24px");
    println!("Line height (normal): {}", metrics.height);
    println!("Glyph height: {}", metrics.glyph_height);
    println!("Ascent: {}", metrics.ascent);
    println!("Descent: {}", metrics.descent);
    println!("Expected: 27px");
    println!(
        "Match: {}",
        if metrics.height == 27.0 { "✅" } else { "❌" }
    );

    // Test with 16px and explicit line-height
    style.font_size = 16.0;
    style.line_height = Some(24.0);

    let metrics2 = measure_text("Test", &style);
    println!("\nFont size: 16px, line-height: 24px");
    println!("Line height: {}", metrics2.height);
    println!("Expected: 24px");
    println!(
        "Match: {}",
        if metrics2.height == 24.0 {
            "✅"
        } else {
            "❌"
        }
    );
}
