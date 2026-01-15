fn main() {
    use css_orchestrator::style_model::ComputedStyle;
    use css_text::measurement::measure_text;
    let style = ComputedStyle {
        font_size: 16.0,
        ..Default::default()
    };
    let m = measure_text("Test", &style);
    println!(
        "glyph_height={}, height={}, ascent={}, descent={}",
        m.glyph_height, m.height, m.ascent, m.descent
    );
}
