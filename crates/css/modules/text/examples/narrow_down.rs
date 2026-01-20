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

    // The 0.875px difference is in "Tab "
    // Let's see if it's in "Tab" itself or the interaction with space

    println!("=== Individual measurements ===");
    let t = measure(&mut font_system, "T");
    let a = measure(&mut font_system, "a");
    let b = measure(&mut font_system, "b");
    let space = measure(&mut font_system, " ");

    println!("T: {:.4}px", t);
    println!("a: {:.4}px", a);
    println!("b: {:.4}px", b);
    println!("space: {:.4}px", space);
    println!("Sum (T+a+b+space): {:.4}px", t + a + b + space);

    println!("\n=== Actual measurements ===");
    let tab = measure(&mut font_system, "Tab");
    let tab_space = measure(&mut font_system, "Tab ");

    println!("'Tab': {:.4}px", tab);
    println!("'Tab ': {:.4}px", tab_space);

    println!("\n=== Kerning analysis ===");
    let expected_tab = t + a + b;
    let expected_tab_space = t + a + b + space;

    println!("Expected 'Tab' (no kerning): {:.4}px", expected_tab);
    println!("Actual 'Tab': {:.4}px", tab);
    println!("Kerning in 'Tab': {:.4}px", tab - expected_tab);

    println!(
        "\nExpected 'Tab ' (no kerning): {:.4}px",
        expected_tab_space
    );
    println!("Actual 'Tab ': {:.4}px", tab_space);
    println!("Kerning in 'Tab ': {:.4}px", tab_space - expected_tab_space);

    println!("\n=== Chrome comparison ===");
    println!("Chrome expects 'Tab ': 26.8828px");
    println!("We measure 'Tab ': {:.4}px", tab_space);
    println!("Difference: {:.4}px", tab_space - 26.8828);

    // Check if Chrome might have different individual char widths
    println!("\n=== If Chrome has 0.875px less in 'Tab ' ===");
    println!("Chrome 'Tab ' = 26.8828px");
    println!("If equally distributed: each char -{:.4}px", 0.875 / 4.0);
    println!("If in space only: space would be {:.4}px", space - 0.875);
    println!("If in 'Tab' only: 'Tab' would be {:.4}px", tab - 0.875);
}
