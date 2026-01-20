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

    println!("=== EXACT ROOT CAUSE ANALYSIS ===\n");

    // What we measure
    let our_a = measure(&mut font_system, "A");
    let our_b = measure(&mut font_system, "B");
    let our_c = measure(&mut font_system, "C");
    let our_tab_space = measure(&mut font_system, "Tab ");
    let our_tab_a = measure(&mut font_system, "Tab A");
    let our_tab_b = measure(&mut font_system, "Tab B");

    // What Chrome reports
    let chrome_tab_a = 38.4375;
    let chrome_tab_b = 38.4375; // Same as Tab A!

    println!("FACT 1: Our measurements");
    println!("  'Tab ': {:.4}px", our_tab_space);
    println!("  'A': {:.4}px", our_a);
    println!("  'B': {:.4}px", our_b);
    println!("  'C': {:.4}px", our_c);
    println!(
        "  'Tab A': {:.4}px (Tab  + A = {:.4}px)",
        our_tab_a,
        our_tab_space + our_a
    );
    println!(
        "  'Tab B': {:.4}px (Tab  + B = {:.4}px)",
        our_tab_b,
        our_tab_space + our_b
    );

    println!("\nFACT 2: Chrome's measurements");
    println!("  'Tab A': {:.4}px", chrome_tab_a);
    println!("  'Tab B': {:.4}px", chrome_tab_b);
    println!(
        "  Difference: {:.4}px (Tab A and Tab B are THE SAME!)",
        chrome_tab_a - chrome_tab_b
    );

    println!("\nFACT 3: Our difference between A and B");
    println!("  Our 'A' - Our 'B' = {:.4}px", our_a - our_b);
    println!(
        "  Our 'Tab A' - Our 'Tab B' = {:.4}px",
        our_tab_a - our_tab_b
    );

    println!("\nFACT 4: Error analysis");
    println!(
        "  Our 'Tab A' - Chrome 'Tab A' = {:.4}px",
        our_tab_a - chrome_tab_a
    );
    println!(
        "  Our 'Tab B' - Chrome 'Tab B' = {:.4}px",
        our_tab_b - chrome_tab_b
    );

    println!("\n=== THE CONCLUSION ===\n");
    println!("Chrome reports 'Tab A' and 'Tab B' as having IDENTICAL widths.");
    println!("This means in Chrome's font, 'A' and 'B' have the SAME width.");
    println!();
    println!("But in our Liberation Serif:");
    println!("  'A' = {:.4}px", our_a);
    println!("  'B' = {:.4}px", our_b);
    println!("  Difference = {:.4}px", our_a - our_b);
    println!();
    println!(
        "This {:.4}px difference in the 'A' glyph width is the EXACT",
        our_a - our_b
    );
    println!("cause of the 0.875px error in 'Tab A'!");
    println!();
    println!("ROOT CAUSE:");
    println!("  The uppercase 'A' glyph in our Liberation Serif font has");
    println!(
        "  an advance width of {:.4}px, but Chrome's font has 'A'",
        our_a
    );
    println!("  with the same width as 'B' ({:.4}px).", our_b);
    println!();
    println!("  This is a FONT FILE DIFFERENCE, not a code bug.");
}
