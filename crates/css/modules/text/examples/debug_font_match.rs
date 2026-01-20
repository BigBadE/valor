use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    let mut font_system = FontSystem::new();
    let buffer_metrics = Metrics::new(16.0, 18.0);

    println!("=== Testing font matching ===\n");

    for font_name in ["Times New Roman", "Liberation Serif", "serif"] {
        println!("Requesting: {}", font_name);

        let attrs = Attrs::new()
            .family(Family::Name(font_name))
            .weight(Weight(400));

        // Check what font actually matched
        let matches = font_system.get_font_matches(&attrs);
        println!("  Matched {} fonts", matches.len());
        if let Some(first) = matches.first() {
            if let Some(font) = font_system.get_font(first.id) {
                let font_ref = font.as_swash();
                let attrs = font_ref.attributes();
                println!("  Font attributes: {:?}", attrs);

                // Try to get font family name from font data
                // This is tricky with swash, but we can at least check metrics
                let metrics = font_ref.metrics(&[]);
                println!("  Units per em: {}", metrics.units_per_em);
            }
        }

        // Measure text
        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, "Tab A", &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        let width = buffer.line_layout(&mut font_system, 0).unwrap()[0].w;

        println!("  'Tab A' width: {:.4}px\n", width);
    }

    println!("Chrome expects: 38.4375px for Times New Roman");
}
