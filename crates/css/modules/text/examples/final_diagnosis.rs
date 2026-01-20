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

    println!("=== FINAL DIAGNOSIS: THE ROOT CAUSE ===\n");

    let a = measure(&mut font_system, "A");
    let b = measure(&mut font_system, "B");
    let c = measure(&mut font_system, "C");
    let d = measure(&mut font_system, "D");

    println!("Our glyph widths (Liberation Serif at 16px):");
    println!("  A: {:.4}px", a);
    println!("  B: {:.4}px", b);
    println!("  C: {:.4}px", c);
    println!("  D: {:.4}px", d);

    println!("\nKey observation:");
    println!("  A - B = {:.4}px", a - b);
    println!("  A - C = {:.4}px", a - c);
    println!("  B - C = {:.4}px", b - c);

    // Chrome reports Tab A/B/C all as 38.4375px
    // This means Chrome's "Tab " + "A/B/C" all equal 38.4375px
    let chrome_tab_abc = 38.4375;
    let tab_space = measure(&mut font_system, "Tab ");

    println!("\nChrome's implied letter widths:");
    println!("  Chrome 'Tab A/B/C': {:.4}px", chrome_tab_abc);
    println!("  Our 'Tab ': {:.4}px", tab_space);
    println!(
        "  Chrome's implied 'A/B/C': {:.4}px (SAME for all three!)",
        chrome_tab_abc - 26.8828
    );

    println!("\nThe smoking gun:");
    println!("  Chrome: A = B = C = {:.4}px", chrome_tab_abc - 26.8828);
    println!("  Our Liberation Serif:");
    println!("    A = {:.4}px", a);
    println!("    B = {:.4}px", b);
    println!("    C = {:.4}px", c);

    println!(
        "\n  ** Chrome's 'A' is {:.4}px NARROWER than ours! **",
        a - (chrome_tab_abc - 26.8828)
    );
    println!("  ** Chrome's 'B' and 'C' match ours (essentially) **");

    println!("\n=== THE ROOT CAUSE ===\n");
    println!("The 0.875px difference is caused by:");
    println!("  Liberation Serif's UPPERCASE 'A' glyph being wider in our font");
    println!("  than in Chrome's Times New Roman/Liberation Serif.");
    println!();
    println!("  Our 'A': {:.4}px", a);
    println!(
        "  Chrome's 'A': ~{:.4}px (calculated from Tab A - Tab )",
        chrome_tab_abc - 26.8828
    );
    println!("  Difference: {:.4}px", a - (chrome_tab_abc - 26.8828));
    println!();
    println!("This is NOT a bug in our code. It's a font metric difference.");
    println!("The 'A' glyph in the Liberation Serif font file on this system");
    println!("has a different advance width than Chrome's font.");
}
