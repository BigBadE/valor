use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn measure(font_system: &mut FontSystem, text: &str, family: &str) -> f32 {
    let attrs = Attrs::new()
        .family(Family::Name(family))
        .weight(Weight(400));
    let buffer_metrics = Metrics::new(16.0, 19.2);
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

    // Test characters that are typically monospace (numbers, common symbols)
    let chars = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];

    let fonts = ["Liberation Serif", "DejaVu Serif", "Times New Roman"];

    for font in &fonts {
        println!("\n{}", font);
        println!("{}", "=".repeat(font.len()));

        let mut widths = std::collections::HashMap::new();

        for &ch in &chars {
            let s = ch.to_string();
            let width = measure(&mut font_system, &s, font);
            println!("  '{}' → {:.4}px", ch, width);

            *widths.entry((width * 10000.0) as i32).or_insert(0) += 1;
        }

        println!("\n  Unique widths: {}", widths.len());
        if widths.len() < chars.len() / 2 {
            println!("  ⚠ WARNING: Too few unique widths!");
        }
    }
}
