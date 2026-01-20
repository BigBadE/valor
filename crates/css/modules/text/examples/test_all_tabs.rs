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

    println!("=== ALL TAB VARIATIONS ===\n");

    // From the HTML, we have Tab A, Tab B, Tab C, Tab D, Tab E
    // Chrome reported all of Tab A/B/C as having the same parent width

    let tests = [
        ("Tab A", Some(38.4375)), // Known from layout JSON
        ("Tab B", Some(38.4375)), // Known - same width as Tab A in Chrome
        ("Tab C", Some(38.4375)), // Known - same width as Tab A in Chrome
        ("Tab D", Some(39.3125)), // From earlier testing - matches our width!
        ("Tab E", Some(37.5312)), // From earlier testing
        ("Tab", None),
        ("Tab ", None),
    ];

    println!("Comparing our measurements with Chrome:\n");
    println!(
        "{:10} {:12} {:12} {:12}",
        "Text", "Our Width", "Chrome", "Difference"
    );
    println!("{}", "-".repeat(50));

    for (text, chrome_width) in tests {
        let our_width = measure(&mut font_system, text);
        if let Some(chrome) = chrome_width {
            let diff = our_width - chrome;
            println!(
                "{:10} {:12.4} {:12.4} {:12.4}",
                text, our_width, chrome, diff
            );
        } else {
            println!("{:10} {:12.4} {:12} {:12}", text, our_width, "N/A", "N/A");
        }
    }

    println!("\n=== PATTERN ANALYSIS ===\n");

    // Check if all variations have the same error
    let tab_a = measure(&mut font_system, "Tab A");
    let tab_b = measure(&mut font_system, "Tab B");
    let tab_c = measure(&mut font_system, "Tab C");
    let tab_d = measure(&mut font_system, "Tab D");
    let tab_e = measure(&mut font_system, "Tab E");

    println!("Our measurements:");
    println!("  Tab A: {:.4}px", tab_a);
    println!("  Tab B: {:.4}px", tab_b);
    println!("  Tab C: {:.4}px", tab_c);
    println!("  Tab D: {:.4}px", tab_d);
    println!("  Tab E: {:.4}px", tab_e);

    println!("\nChrome's measurements:");
    println!("  Tab A: 38.4375px");
    println!("  Tab B: 38.4375px");
    println!("  Tab C: 38.4375px");
    println!("  Tab D: 39.3125px");
    println!("  Tab E: 37.5312px");

    println!("\nDifferences:");
    println!("  Tab A: {:.4}px", tab_a - 38.4375);
    println!("  Tab B: {:.4}px", tab_b - 38.4375);
    println!("  Tab C: {:.4}px", tab_c - 38.4375);
    println!("  Tab D: {:.4}px", tab_d - 39.3125);
    println!("  Tab E: {:.4}px", tab_e - 37.5312);

    // Check individual letters
    println!("\n=== INDIVIDUAL LETTER ANALYSIS ===\n");
    let a_char = measure(&mut font_system, "A");
    let b_char = measure(&mut font_system, "B");
    let c_char = measure(&mut font_system, "C");
    let d_char = measure(&mut font_system, "D");
    let e_char = measure(&mut font_system, "E");

    println!("Individual uppercase letters:");
    println!("  A: {:.4}px", a_char);
    println!("  B: {:.4}px", b_char);
    println!("  C: {:.4}px", c_char);
    println!("  D: {:.4}px", d_char);
    println!("  E: {:.4}px", e_char);

    let tab_space = measure(&mut font_system, "Tab ");
    println!("\n'Tab ' measurements:");
    println!("  Our 'Tab ': {:.4}px", tab_space);
    println!("  Chrome 'Tab ': 26.8828px (implied from Tab A)");
    println!("  Difference: {:.4}px", tab_space - 26.8828);

    // Compute what each Tab X should be
    println!("\nExpected widths (our 'Tab ' + letter):");
    println!(
        "  Tab A should be: {:.4}px (actual: {:.4}px)",
        tab_space + a_char,
        tab_a
    );
    println!(
        "  Tab B should be: {:.4}px (actual: {:.4}px)",
        tab_space + b_char,
        tab_b
    );
    println!(
        "  Tab C should be: {:.4}px (actual: {:.4}px)",
        tab_space + c_char,
        tab_c
    );
    println!(
        "  Tab D should be: {:.4}px (actual: {:.4}px)",
        tab_space + d_char,
        tab_d
    );
    println!(
        "  Tab E should be: {:.4}px (actual: {:.4}px)",
        tab_space + e_char,
        tab_e
    );
}
