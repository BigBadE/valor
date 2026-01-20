use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    let mut font_system = FontSystem::new();

    // Test Liberation Serif (default)
    println!("Testing 'Tab A' at 16px:");
    println!();

    let fonts = vec![
        ("Liberation Serif", Family::Name("Liberation Serif")),
        ("Liberation Sans", Family::Name("Liberation Sans")),
        ("DejaVu Serif", Family::Name("DejaVu Serif")),
        ("DejaVu Sans", Family::Name("DejaVu Sans")),
    ];

    for (name, family) in fonts {
        let attrs = Attrs::new().family(family).weight(Weight(400));

        let buffer_metrics = Metrics::new(16.0, 18.0);
        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);

        buffer.set_text(&mut font_system, "Tab A", &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);

        for line_idx in 0..buffer.lines.len() {
            if let Some(layout_lines) = buffer.line_layout(&mut font_system, line_idx) {
                for layout_line in layout_lines {
                    println!("  {:20} width: {:.4}px", name, layout_line.w);
                }
            }
        }
    }

    println!();
    println!("Chrome expects: ~38.4px (60.4px total - 22px padding/border)");
    println!("Valor produces: ~61.3px (83.3px total - 22px padding/border)");
}
