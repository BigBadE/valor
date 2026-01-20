use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn measure(font_system: &mut FontSystem, text: &str) -> f32 {
    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(400));
    let buffer_metrics = Metrics::new(16.0, 18.0);
    let mut buffer = Buffer::new(font_system, buffer_metrics);
    buffer.set_text(font_system, text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    buffer.line_layout(font_system, 0).unwrap()[0].w
}

fn main() {
    let mut font_system = FontSystem::new();

    println!("=== CHECKING ALL UPPERCASE LETTERS IN LIBERATION SERIF ===\n");

    // Measure all uppercase letters
    let letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut widths = Vec::new();

    println!("Individual letter widths:");
    for ch in letters.chars() {
        let width = measure(&mut font_system, &ch.to_string());
        widths.push((ch, width));
        print!("{}: {:.4}px  ", ch, width);
        if (ch as u8 - b'A' + 1) % 5 == 0 {
            println!();
        }
    }
    println!("\n");

    // Find letters with similar widths to 'A'
    let a_width = widths.iter().find(|(ch, _)| *ch == 'A').unwrap().1;
    let b_width = widths.iter().find(|(ch, _)| *ch == 'B').unwrap().1;

    println!("Letters grouped by width:");
    println!("\nLetters with width ~{:.4}px (like 'B'):", b_width);
    for (ch, width) in &widths {
        if (width - b_width).abs() < 0.01 {
            print!("{} ", ch);
        }
    }

    println!("\n\nLetters with width ~{:.4}px (like 'A'):", a_width);
    for (ch, width) in &widths {
        if (width - a_width).abs() < 0.01 {
            print!("{} ", ch);
        }
    }

    // Test each letter with "Tab "
    println!("\n\n=== TESTING 'Tab X' FOR ALL LETTERS ===\n");
    println!("If Chrome treats all letters the same as 'B' (width ~10.67px),");
    println!("then we should see errors for letters that don't match that width.\n");

    let tab_space = measure(&mut font_system, "Tab ");
    println!("Our 'Tab ': {:.4}px", tab_space);
    println!("Chrome's implied 'Tab ': ~27.76px (from Tab B and Tab D)\n");

    println!(
        "{:>6} {:>10} {:>12} {:>15} {:>15}",
        "Letter", "Width", "Tab X", "Chrome if B-like", "Expected Error"
    );
    println!("{}", "-".repeat(70));

    for (ch, width) in &widths {
        let tab_x = measure(&mut font_system, &format!("Tab {}", ch));

        // If Chrome treats this letter like 'B' (10.6719px)
        let chrome_if_b_like = 27.76 + 10.6719;
        let expected_error = tab_x - chrome_if_b_like;

        let marker = if expected_error.abs() > 0.5 {
            " ← LARGE ERROR"
        } else {
            ""
        };

        println!(
            "{:>6} {:10.4} {:12.4} {:15.4} {:15.4}{}",
            ch, width, tab_x, chrome_if_b_like, expected_error, marker
        );
    }

    println!("\n=== CHARACTERS WITH POTENTIALLY WRONG WIDTHS ===\n");
    println!("Characters where our width differs significantly from 'B'-like width:");

    for (ch, width) in &widths {
        let diff_from_b = width - b_width;
        if diff_from_b.abs() > 0.5 {
            println!(
                "  '{}': {:.4}px (diff from B: {:+.4}px)",
                ch, width, diff_from_b
            );
        }
    }

    println!("\n=== SUMMARY ===");
    println!("If Chrome uses a font where most uppercase letters have similar widths,");
    println!("but our Liberation Serif has different widths for some letters,");
    println!("those letters will cause test failures.");
}
