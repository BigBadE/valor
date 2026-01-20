use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn measure(font_system: &mut FontSystem, text: &str, font_name: &str) -> f32 {
    let attrs = Attrs::new()
        .family(Family::Name(font_name))
        .weight(Weight(400));
    let buffer_metrics = Metrics::new(16.0, 18.0);
    let mut buffer = Buffer::new(font_system, buffer_metrics);
    buffer.set_text(font_system, text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    buffer.line_layout(font_system, 0).unwrap()[0].w
}

fn main() {
    let mut font_system = FontSystem::new();

    println!("=== TESTING MONOSPACED FONTS ===\n");

    let monospace_fonts = [
        "Liberation Mono",
        "Courier New",
        "DejaVu Sans Mono",
        "Noto Sans Mono",
        "Consolas",
        "Monaco",
    ];

    println!("In a TRUE monospaced font, ALL letters should have IDENTICAL widths.\n");

    for font in &monospace_fonts {
        println!("Font: {}", font);
        println!("{}", "=".repeat(60));

        let letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let mut widths = Vec::new();

        for ch in letters.chars() {
            let width = measure(&mut font_system, &ch.to_string(), font);
            widths.push((ch, width));
        }

        // Print sample widths
        print!("Sample: ");
        for (ch, width) in widths.iter().take(10) {
            print!("{}:{:.2} ", ch, width);
        }
        println!();

        // Check if all widths are the same
        let first_width = widths[0].1;
        let all_same = widths.iter().all(|(_, w)| (w - first_width).abs() < 0.01);

        if all_same {
            println!("✓ TRUE MONOSPACE: All letters = {:.4}px", first_width);
        } else {
            println!("✗ NOT MONOSPACE: Letters have different widths");

            // Show the variation
            let min_width = widths
                .iter()
                .map(|(_, w)| w)
                .fold(f32::INFINITY, |a, &b| a.min(b));
            let max_width = widths
                .iter()
                .map(|(_, w)| w)
                .fold(f32::NEG_INFINITY, |a, &b| a.max(b));
            println!(
                "  Range: {:.4}px to {:.4}px (variation: {:.4}px)",
                min_width,
                max_width,
                max_width - min_width
            );

            // Count unique widths
            let mut unique_widths = widths
                .iter()
                .map(|(_, w)| format!("{:.4}", w))
                .collect::<Vec<_>>();
            unique_widths.sort();
            unique_widths.dedup();
            println!("  Unique widths: {}", unique_widths.len());
        }

        // Test Tab A
        let tab_a = measure(&mut font_system, "Tab A", font);
        let tab_b = measure(&mut font_system, "Tab B", font);

        println!("Tab A: {:.4}px", tab_a);
        println!("Tab B: {:.4}px", tab_b);
        println!("Difference: {:.4}px", tab_a - tab_b);

        if (tab_a - tab_b).abs() < 0.01 {
            println!("✓ Tab A and Tab B are the SAME (as expected for monospace)");
        } else {
            println!("✗ Tab A and Tab B are DIFFERENT (unexpected for monospace!)");
        }

        println!();
    }

    println!("\n=== COMPARING MONOSPACE TO PROPORTIONAL ===\n");

    let test_fonts = [
        ("Liberation Mono", true),
        ("Liberation Serif", false),
        ("DejaVu Sans Mono", true),
        ("DejaVu Serif", false),
    ];

    println!(
        "{:25} {:>8} {:>8} {:>12} {:>15}",
        "Font", "A", "B", "A - B", "Status"
    );
    println!("{}", "-".repeat(75));

    for (font, is_mono) in &test_fonts {
        let a = measure(&mut font_system, "A", font);
        let b = measure(&mut font_system, "B", font);
        let diff = a - b;

        let expected = if *is_mono { "Should be 0" } else { "Can vary" };
        let status = if *is_mono && diff.abs() < 0.01 {
            "✓ Monospace"
        } else if !is_mono {
            "Proportional"
        } else {
            "✗ Not mono!"
        };

        println!(
            "{:25} {:8.4} {:8.4} {:12.4} {:>15}",
            font, a, b, diff, status
        );
    }

    println!("\n=== ANSWER: WOULD MONOSPACE FIX THE ISSUE? ===\n");

    let mono_a = measure(&mut font_system, "A", "Liberation Mono");
    let mono_b = measure(&mut font_system, "B", "Liberation Mono");

    println!("Liberation Mono:");
    println!("  A: {:.4}px", mono_a);
    println!("  B: {:.4}px", mono_b);
    println!("  Difference: {:.4}px", mono_a - mono_b);

    if (mono_a - mono_b).abs() < 0.01 {
        println!("\n✓ YES! In a monospace font, A and B have the same width.");
        println!("  This would eliminate the 0.875px error we see with Liberation Serif.");
        println!("  However, Chrome is not using a monospace font for the test.");
        println!("  Chrome expects a proportional serif font where all letters");
        println!("  have reasonable varied widths (not Liberation Serif's abnormal widths).");
    } else {
        println!("\n✗ Even the monospace font has variation!");
    }
}
