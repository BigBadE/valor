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

    println!("=== Detailed breakdown ===");

    // Chrome expects Tab A = 38.4375
    // Chrome expects Tab D = 39.3125
    // Difference = 0.875

    // We measure both as 39.3125
    // So Chrome measures Tab A as 0.875 narrower

    // Working backwards from Chrome's Tab A = 38.4375:
    // If 'A' = 11.5547 (we measure this)
    // Then Chrome's "Tab " = 38.4375 - 11.5547 = 26.8828
    // We measure "Tab " = 27.7578
    // Difference = 0.875px

    let tab_space = measure(&mut font_system, "Tab ");
    let a_char = measure(&mut font_system, "A");

    println!("Our 'Tab ': {:.4}px", tab_space);
    println!("Our 'A':    {:.4}px", a_char);
    println!("Our total:  {:.4}px", tab_space + a_char);
    println!();
    println!("Chrome expects 'Tab A': 38.4375px");
    println!(
        "So Chrome's 'Tab ' would be: {:.4}px (if A={:.4})",
        38.4375 - a_char,
        a_char
    );
    println!(
        "Difference in 'Tab ':  {:.4}px",
        tab_space - (38.4375 - a_char)
    );

    println!("\n=== Testing if the issue is in 'Tab' or the space ===");
    let tab = measure(&mut font_system, "Tab");
    let space = measure(&mut font_system, " ");
    println!("'Tab':  {:.4}px", tab);
    println!("' ':    {:.4}px", space);
    println!("Sum:    {:.4}px", tab + space);
    println!("Actual 'Tab ': {:.4}px", tab_space);
    println!("Match: {}", (tab + space - tab_space).abs() < 0.001);
}
