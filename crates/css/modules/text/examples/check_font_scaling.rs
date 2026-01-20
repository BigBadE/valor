use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    let mut font_system = FontSystem::new();

    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(400));

    println!("=== Testing different font sizes ===");
    // Chrome expects 26.8828px for "Tab " at font-size 16px
    // We get 27.7578px
    // What if we used a slightly smaller font size?

    for font_size in [15.0, 15.5, 15.75, 16.0] {
        let buffer_metrics = Metrics::new(font_size, font_size * 1.125);
        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, "Tab ", &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);

        let width = buffer
            .layout_runs()
            .flat_map(|run| run.glyphs.iter())
            .map(|g| g.w)
            .sum::<f32>();

        println!("font-size {:.2}px → 'Tab ' = {:.4}px", font_size, width);
    }

    println!("\n=== Reverse calculation ===");
    // If Chrome's 26.8828px is correct, what font size would give us that?
    // 27.7578px at 16px
    // 26.8828px at ?px
    let ratio = 26.8828 / 27.7578;
    let implied_font_size = 16.0 * ratio;
    println!("If Chrome's measurement is proportional:");
    println!("  Ratio: {:.6}", ratio);
    println!("  Implied font-size: {:.4}px", implied_font_size);

    // Test that font size
    let buffer_metrics = Metrics::new(implied_font_size, implied_font_size * 1.125);
    let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
    buffer.set_text(&mut font_system, "Tab ", &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(&mut font_system, false);

    let width = buffer
        .layout_runs()
        .flat_map(|run| run.glyphs.iter())
        .map(|g| g.w)
        .sum::<f32>();

    println!(
        "  Actual 'Tab ' at {:.4}px font-size: {:.4}px",
        implied_font_size, width
    );
    println!("  Target: 26.8828px");
    println!("  Difference: {:.4}px", width - 26.8828);
}
