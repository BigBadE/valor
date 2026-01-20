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

    let monospace_fonts = ["Liberation Mono", "DejaVu Sans Mono", "Courier New"];

    // Test all uppercase letters in monospace fonts
    let letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

    for font in &monospace_fonts {
        println!("\n{}", font);
        println!("{}", "=".repeat(font.len()));

        let mut widths = Vec::new();

        for ch in letters.chars() {
            let width = measure(&mut font_system, &ch.to_string(), font);
            widths.push(width);
            println!("  '{}' → {:.4}px", ch, width);
        }

        // Check if all widths are identical
        let first = widths[0];
        let all_same = widths.iter().all(|&w| (w - first).abs() < 0.001);

        let unique: std::collections::HashSet<_> =
            widths.iter().map(|&w| (w * 10000.0) as i32).collect();

        println!(
            "\n  All characters same width? {}",
            if all_same { "YES ✓" } else { "NO ✗" }
        );
        println!("  Unique widths: {}", unique.len());

        if !all_same {
            println!("  ⚠ WARNING: Not actually monospace!");
            let min = widths.iter().cloned().fold(f32::INFINITY, f32::min);
            let max = widths.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            println!(
                "  Range: {:.4}px - {:.4}px (diff: {:.4}px)",
                min,
                max,
                max - min
            );
        }
    }
}
