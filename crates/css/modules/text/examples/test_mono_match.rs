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

    // Since Liberation Mono has all characters at 9.6016px,
    // we can predict what Chrome should expect for any text

    let char_width = 9.6016;

    let tests = [
        ("A", 1),
        ("AB", 2),
        ("ABC", 3),
        ("Tab A", 5), // T, a, b, space, A = 5 chars
        ("Hello", 5),
    ];

    println!(
        "Liberation Mono @ 16px (each char = {:.4}px):\n",
        char_width
    );

    for (text, num_chars) in &tests {
        let measured = measure(&mut font_system, text, "Liberation Mono", 16.0);
        let expected = char_width * (*num_chars as f32);
        let diff = measured - expected;

        let match_status = if diff.abs() < 0.01 {
            "✓ EXACT"
        } else if diff.abs() < 0.1 {
            "~ Close"
        } else {
            "✗ OFF"
        };

        println!("  \"{}\" ({} chars)", text, num_chars);
        println!("    Measured:  {:.4}px", measured);
        println!("    Expected:  {:.4}px", expected);
        println!("    Diff:      {:+.4}px  {}", diff, match_status);
        println!();
    }

    println!("\nNOTE: We don't have Chrome's actual expectations for monospace text,");
    println!("since the test fixtures use proportional fonts (Liberation Serif).");
    println!("But IF Chrome uses Liberation Mono, our measurements should be exact.");
}
