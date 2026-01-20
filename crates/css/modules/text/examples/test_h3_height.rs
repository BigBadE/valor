use css_orchestrator::style_model::ComputedStyle;
use css_text::measurement::measure_text;

fn main() {
    // h3 with Liberation Serif Bold at 18.72px
    let style = ComputedStyle {
        font_size: 18.72,
        font_weight: 700,
        font_family: None, // Will use default (Liberation Serif)
        line_height: None, // normal
        ..Default::default()
    };

    let text = "Test 1: Items should size to content (not equal 160px each)";
    let metrics = measure_text(text, &style);

    println!("h3 text measurement:");
    println!("  font_size: 18.72px");
    println!("  font_weight: 700");
    println!("  text height: {:.0}px", metrics.height);
    println!("  text width: {:.2}px", metrics.width);
    println!("  glyph_height: {:.2}px", metrics.glyph_height);
    println!("  ascent: {:.2}px", metrics.ascent);
    println!("  descent: {:.2}px", metrics.descent);
    println!();
    println!("Chrome expects:");
    println!("  h3 rect height: 22px");
    println!("  text rect height: 21px");
    println!();
    println!("Valor produces:");
    println!("  text height: {:.0}px", metrics.height);
    println!("  Difference: {:.0}px", metrics.height - 21.0);
}
