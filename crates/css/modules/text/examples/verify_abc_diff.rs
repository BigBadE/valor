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

    println!("=== VERIFYING THE A vs B/C DISCREPANCY ===\n");

    // Measure multiple times to ensure consistency
    println!("Measuring each text 3 times:");
    for text in ["Tab A", "Tab B", "Tab C"] {
        print!("{:10} → ", text);
        for _ in 0..3 {
            let width = measure(&mut font_system, text);
            print!("{:.4}px  ", width);
        }
        println!();
    }

    println!("\n=== INDIVIDUAL COMPONENTS ===\n");

    let tab_space = measure(&mut font_system, "Tab ");
    let a = measure(&mut font_system, "A");
    let b = measure(&mut font_system, "B");
    let c = measure(&mut font_system, "C");

    println!("'Tab ': {:.4}px", tab_space);
    println!("'A': {:.4}px", a);
    println!("'B': {:.4}px", b);
    println!("'C': {:.4}px", c);

    println!("\nExpected widths (Tab  + letter):");
    println!(
        "  Tab A: {:.4}px (actual: {:.4}px, diff: {:.4}px)",
        tab_space + a,
        measure(&mut font_system, "Tab A"),
        measure(&mut font_system, "Tab A") - (tab_space + a)
    );
    println!(
        "  Tab B: {:.4}px (actual: {:.4}px, diff: {:.4}px)",
        tab_space + b,
        measure(&mut font_system, "Tab B"),
        measure(&mut font_system, "Tab B") - (tab_space + b)
    );
    println!(
        "  Tab C: {:.4}px (actual: {:.4}px, diff: {:.4}px)",
        tab_space + c,
        measure(&mut font_system, "Tab C"),
        measure(&mut font_system, "Tab C") - (tab_space + c)
    );

    println!("\n=== THE MYSTERY ===\n");
    println!("Chrome reports:");
    println!("  Tab A: 38.4375px");
    println!("  Tab B: 38.4375px (SAME as Tab A)");
    println!("  Tab C: 38.4375px (SAME as Tab A)");

    let tab_a = measure(&mut font_system, "Tab A");
    let tab_b = measure(&mut font_system, "Tab B");
    let tab_c = measure(&mut font_system, "Tab C");

    println!("\nWe measure:");
    println!("  Tab A: {:.4}px", tab_a);
    println!("  Tab B: {:.4}px", tab_b);
    println!("  Tab C: {:.4}px", tab_c);

    println!("\nDifferences:");
    println!("  Tab A vs Tab B: {:.4}px", tab_a - tab_b);
    println!("  Tab A vs Tab C: {:.4}px", tab_a - tab_c);
    println!("  Tab B vs Tab C: {:.4}px", tab_b - tab_c);

    println!("\n** WHY ARE Tab A and Tab B/C different in our measurements? **");
    println!(
        "** A and B have different widths: A={:.4}px, B={:.4}px, diff={:.4}px **",
        a,
        b,
        a - b
    );
    println!("** But Chrome reports Tab A/B/C as having the SAME width! **");
}
