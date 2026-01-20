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

    let fonts = [
        "Liberation Serif",
        "Liberation Sans",
        "Times New Roman",
        "Arial",
        "DejaVu Sans",
        "DejaVu Serif",
        "Noto Sans",
        "Noto Serif",
    ];

    let test_letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

    println!("=== COMPARING UPPERCASE LETTER WIDTHS ACROSS FONTS ===\n");

    for font in &fonts {
        println!("Font: {}", font);
        println!("{}", "=".repeat(60));

        let mut widths = Vec::new();
        for ch in test_letters.chars() {
            let width = measure(&mut font_system, &ch.to_string(), font);
            widths.push((ch, width));
        }

        // Print all widths compactly
        for (i, (ch, width)) in widths.iter().enumerate() {
            print!("{}:{:6.2}  ", ch, width);
            if (i + 1) % 6 == 0 {
                println!();
            }
        }
        if widths.len() % 6 != 0 {
            println!();
        }

        // Check for duplicate widths
        let mut width_groups: std::collections::HashMap<String, Vec<char>> =
            std::collections::HashMap::new();
        for (ch, width) in &widths {
            let key = format!("{:.4}", width);
            width_groups.entry(key).or_insert_with(Vec::new).push(*ch);
        }

        // Find groups with multiple letters
        let mut large_groups: Vec<_> = width_groups
            .iter()
            .filter(|(_, chars)| chars.len() > 1)
            .collect();
        large_groups.sort_by_key(|(width_str, _)| width_str.parse::<f32>().unwrap_or(0.0) as i32);
        large_groups.reverse();

        if !large_groups.is_empty() {
            println!("\nLetters sharing same width:");
            for (width_str, chars) in large_groups {
                if chars.len() > 2 {
                    println!(
                        "  {}: {} letters → {}",
                        width_str,
                        chars.len(),
                        chars.iter().collect::<String>()
                    );
                }
            }
        }

        // Count unique widths
        let unique_widths = width_groups.len();
        println!("Unique widths: {} out of 26 letters", unique_widths);

        // Check if this looks like a proportional font
        if unique_widths > 20 {
            println!("✓ Appears to be a proper proportional font");
        } else if unique_widths < 15 {
            println!("⚠ WARNING: Too few unique widths! Possibly corrupted font");
        } else {
            println!("△ Moderate variation in widths");
        }

        println!();
    }

    println!("\n=== DETAILED COMPARISON: A vs B WIDTH ===\n");
    println!(
        "{:20} {:>10} {:>10} {:>12} {:>25}",
        "Font", "A", "B", "A - B", "Assessment"
    );
    println!("{}", "-".repeat(80));

    for font in &fonts {
        let a = measure(&mut font_system, "A", font);
        let b = measure(&mut font_system, "B", font);
        let diff = a - b;

        let assessment = if diff.abs() < 0.1 {
            "Nearly identical"
        } else if diff.abs() < 0.5 {
            "Slightly different"
        } else if diff > 0.5 {
            "A much wider (SUSPICIOUS)"
        } else {
            "B much wider"
        };

        println!(
            "{:20} {:10.4} {:10.4} {:12.4} {:>25}",
            font, a, b, diff, assessment
        );
    }

    println!("\n=== TESTING Tab A vs Tab B ===\n");
    println!(
        "{:20} {:>12} {:>12} {:>12} {:>15}",
        "Font", "Tab A", "Tab B", "Difference", "Status"
    );
    println!("{}", "-".repeat(75));

    for font in &fonts {
        let tab_a = measure(&mut font_system, "Tab A", font);
        let tab_b = measure(&mut font_system, "Tab B", font);
        let diff = tab_a - tab_b;

        let status = if diff.abs() < 0.01 {
            "SAME ✓"
        } else if diff.abs() < 0.5 {
            "Close"
        } else {
            "DIFFERENT ✗"
        };

        println!(
            "{:20} {:12.4} {:12.4} {:12.4} {:>15}",
            font, tab_a, tab_b, diff, status
        );
    }

    println!("\n=== CONCLUSION ===\n");
    println!("Liberation Serif analysis:");
    let lib_a = measure(&mut font_system, "A", "Liberation Serif");
    let lib_b = measure(&mut font_system, "B", "Liberation Serif");
    println!("  A width: {:.4}px", lib_a);
    println!("  B width: {:.4}px", lib_b);
    println!("  Difference: {:.4}px", lib_a - lib_b);

    if (lib_a - lib_b).abs() > 0.5 {
        println!("\n⚠ Liberation Serif has ABNORMAL glyph widths!");
        println!("  The 'A' glyph is significantly wider than 'B'.");
        println!("  This is NOT normal for a serif font.");

        println!("\nComparing to other serif fonts:");
        for font in ["Times New Roman", "DejaVu Serif", "Noto Serif"] {
            let a = measure(&mut font_system, "A", font);
            let b = measure(&mut font_system, "B", font);
            println!("  {}: A={:.4}, B={:.4}, diff={:.4}", font, a, b, a - b);
        }
    }
}
