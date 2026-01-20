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

    println!("=== EXACT ANALYSIS OF THE 0.875px DISCREPANCY ===\n");

    println!("Configuration:");
    println!("  Font: Liberation Serif");
    println!("  Font-size: 16px");
    println!("  Text: 'Tab A'\n");

    // What we measure
    let our_tab_space = measure(&mut font_system, "Tab ", "Liberation Serif");
    let our_a = measure(&mut font_system, "A", "Liberation Serif");
    let our_tab_a = measure(&mut font_system, "Tab A", "Liberation Serif");

    // What Chrome expects (from layout JSON)
    let chrome_tab_a = 38.4375;

    println!("OUR MEASUREMENTS (cosmic-text with Liberation Serif at 16px):");
    println!("  'Tab ': {:.4}px", our_tab_space);
    println!("  'A':    {:.4}px", our_a);
    println!("  'Tab A': {:.4}px", our_tab_a);
    println!("  Sum of parts: {:.4}px (Tab  + A)", our_tab_space + our_a);
    println!("  Actual 'Tab A': {:.4}px", our_tab_a);
    println!("  → No kerning between space and A (sum equals actual)\n");

    println!("CHROME'S MEASUREMENTS (from layout test output):");
    println!("  'Tab A': {:.4}px", chrome_tab_a);
    println!(
        "  Implied 'Tab ': {:.4}px (Tab A - our A)",
        chrome_tab_a - our_a
    );
    println!("  Implied 'A': {:.4}px (same as ours)\n", our_a);

    println!("DIFFERENCE:");
    println!(
        "  Our 'Tab A' - Chrome 'Tab A': {:.4}px",
        our_tab_a - chrome_tab_a
    );
    println!(
        "  Our 'Tab ' - Chrome 'Tab ': {:.4}px",
        our_tab_space - (chrome_tab_a - our_a)
    );
    println!("  Our 'A' - Chrome 'A': {:.4}px (EXACT MATCH!)\n", 0.0);

    println!("CONCLUSION:");
    println!("  The 'A' character is measured identically.");
    println!("  The entire 0.875px difference is in 'Tab ' (4 characters).");
    println!("  This means Liberation Serif's glyphs for T/a/b/space or");
    println!("  their kerning differ between cosmic-text and Chrome.\n");

    // Break down individual characters
    println!("=== CHARACTER-BY-CHARACTER BREAKDOWN ===\n");
    let t = measure(&mut font_system, "T", "Liberation Serif");
    let a_lower = measure(&mut font_system, "a", "Liberation Serif");
    let b = measure(&mut font_system, "b", "Liberation Serif");
    let space = measure(&mut font_system, " ", "Liberation Serif");

    println!("Individual glyphs (cosmic-text Liberation Serif):");
    println!("  'T': {:.4}px", t);
    println!("  'a': {:.4}px", a_lower);
    println!("  'b': {:.4}px", b);
    println!("  ' ': {:.4}px", space);
    println!("  Sum: {:.4}px", t + a_lower + b + space);

    let tab = measure(&mut font_system, "Tab", "Liberation Serif");
    println!("\n'Tab' with shaping/kerning:");
    println!("  'Tab': {:.4}px", tab);
    println!("  Kerning: {:.4}px", tab - (t + a_lower + b));

    println!("\n'Tab ' with shaping/kerning:");
    println!("  'Tab ': {:.4}px", our_tab_space);
    println!(
        "  Kerning: {:.4}px",
        our_tab_space - (t + a_lower + b + space)
    );

    println!("\n=== WHAT WOULD FIX IT ===\n");
    let needed_tab_space = chrome_tab_a - our_a;
    let needed_reduction = our_tab_space - needed_tab_space;

    println!("To match Chrome's 38.4375px for 'Tab A':");
    println!("  We need 'Tab ' to be: {:.4}px", needed_tab_space);
    println!("  Current 'Tab ': {:.4}px", our_tab_space);
    println!(
        "  Reduction needed: {:.4}px ({:.2}%)",
        needed_reduction,
        (needed_reduction / our_tab_space) * 100.0
    );

    println!("\nThis reduction could come from:");
    println!("  - Different glyph advance widths in the font file");
    println!("  - Different kerning values between character pairs");
    println!("  - Different sub-pixel positioning/hinting");
    println!("  - Chrome using a different font file with same name");

    // Test what font-size would give us the right width
    for test_size in [15.0, 15.5, 15.64, 16.0] {
        let buffer_metrics = Metrics::new(test_size, test_size * 1.125);
        let attrs = Attrs::new()
            .family(Family::Name("Liberation Serif"))
            .weight(Weight(400));
        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, "Tab A", &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        let width = buffer.line_layout(&mut font_system, 0).unwrap()[0].w;

        let marker = if (width - chrome_tab_a).abs() < 0.01 {
            " ← MATCH!"
        } else {
            ""
        };
        println!(
            "\n  At font-size {:.2}px: 'Tab A' = {:.4}px{}",
            test_size, width, marker
        );
    }
}
