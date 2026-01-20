use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn measure(font_system: &mut FontSystem, text: &str, family: &str, size: f32) -> f32 {
    let attrs = Attrs::new()
        .family(Family::Name(family))
        .weight(Weight(400));
    let buffer_metrics = Metrics::new(size, size * 1.2);
    let mut buffer = Buffer::new(font_system, buffer_metrics);
    buffer.set_text(font_system, text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);

    if let Some(layout) = buffer.line_layout(font_system, 0) {
        let total: f32 = layout.iter().map(|l| l.w).sum();
        total
    } else {
        0.0
    }
}

fn main() {
    let mut font_system = FontSystem::new();

    // Test text from the h3 fixture that we know Chrome expects specific widths for
    // Chrome expects "Tab A" at 16px to be 38.4375px
    let test_cases = [
        ("Tab A", 16.0, 38.4375),
        ("one", 16.0, 23.109375),
        ("two", 16.0, 24.0),
    ];

    let fonts = [
        "Liberation Serif",
        "Liberation Sans",
        "Times New Roman",
        "DejaVu Serif",
        "DejaVu Sans",
        "Noto Serif",
        "Noto Sans",
    ];

    println!("Finding which font matches Chrome's expectations:\n");

    for (text, size, chrome_width) in &test_cases {
        println!(
            "Test: \"{}\" @ {}px (Chrome expects: {}px)",
            text, size, chrome_width
        );

        for font in &fonts {
            let width = measure(&mut font_system, text, font, *size);
            let diff = (width - chrome_width).abs();
            let status = if diff < 0.01 {
                "✓ EXACT MATCH"
            } else if diff < 0.5 {
                "~ Close"
            } else {
                ""
            };

            println!(
                "  {:20} → {:8.4}px  (diff: {:+7.4}px) {}",
                font,
                width,
                width - chrome_width,
                status
            );
        }
        println!();
    }
}
