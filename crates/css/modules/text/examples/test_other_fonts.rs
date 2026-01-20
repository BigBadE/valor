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

    println!("=== TESTING OTHER FONTS ===\n");

    let fonts = [
        "Liberation Serif",
        "Liberation Sans",
        "Times New Roman",
        "Arial",
        "DejaVu Sans",
        "DejaVu Serif",
    ];

    println!("Testing if 'A' has different width than 'B' in various fonts:\n");
    println!(
        "{:20} {:>10} {:>10} {:>10} {:>12}",
        "Font", "A", "B", "A - B", "Tab A"
    );
    println!("{}", "-".repeat(65));

    for font in fonts {
        let a = measure(&mut font_system, "A", font);
        let b = measure(&mut font_system, "B", font);
        let tab_a = measure(&mut font_system, "Tab A", font);
        let diff = a - b;

        let marker = if diff.abs() > 0.5 {
            " ← LARGE DIFF"
        } else {
            ""
        };
        println!(
            "{:20} {:10.4} {:10.4} {:10.4} {:12.4}{}",
            font, a, b, diff, tab_a, marker
        );
    }

    println!("\n=== DETAILED ANALYSIS ===\n");

    for font in fonts {
        println!("Font: {}", font);

        let tab_space = measure(&mut font_system, "Tab ", font);
        let a = measure(&mut font_system, "A", font);
        let b = measure(&mut font_system, "B", font);
        let c = measure(&mut font_system, "C", font);
        let d = measure(&mut font_system, "D", font);

        let tab_a = measure(&mut font_system, "Tab A", font);
        let tab_b = measure(&mut font_system, "Tab B", font);
        let tab_d = measure(&mut font_system, "Tab D", font);

        println!("  Letters: A={:.4}, B={:.4}, C={:.4}, D={:.4}", a, b, c, d);
        println!(
            "  Tab combos: Tab A={:.4}, Tab B={:.4}, Tab D={:.4}",
            tab_a, tab_b, tab_d
        );
        println!("  Tab A - Tab B = {:.4}px", tab_a - tab_b);
        println!("  A - B = {:.4}px", a - b);
        println!(
            "  Match? {}\n",
            if (tab_a - tab_b - (a - b)).abs() < 0.01 {
                "YES"
            } else {
                "NO"
            }
        );
    }

    println!("=== KEY QUESTION ===");
    println!("Does Liberation Serif's 'A' being wider than 'B' occur in other fonts?");
    println!("\nAnswer:");

    let lib_serif_a = measure(&mut font_system, "A", "Liberation Serif");
    let lib_serif_b = measure(&mut font_system, "B", "Liberation Serif");
    let lib_sans_a = measure(&mut font_system, "A", "Liberation Sans");
    let lib_sans_b = measure(&mut font_system, "B", "Liberation Sans");

    println!(
        "  Liberation Serif: A={:.4}, B={:.4}, diff={:.4}",
        lib_serif_a,
        lib_serif_b,
        lib_serif_a - lib_serif_b
    );
    println!(
        "  Liberation Sans: A={:.4}, B={:.4}, diff={:.4}",
        lib_sans_a,
        lib_sans_b,
        lib_sans_a - lib_sans_b
    );

    if (lib_serif_a - lib_serif_b).abs() > 0.5 {
        println!("\nLiberation Serif has the issue (A wider than B by >0.5px)");
    }
    if (lib_sans_a - lib_sans_b).abs() > 0.5 {
        println!("Liberation Sans also has the issue (A wider than B by >0.5px)");
    }
}
