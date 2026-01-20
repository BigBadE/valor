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

    println!("=== TESTING SPACE + A KERNING ===\n");

    let space = measure(&mut font_system, " ");
    let a_upper = measure(&mut font_system, "A");
    let space_a = measure(&mut font_system, " A");

    println!("Individual measurements:");
    println!("  ' ' (space): {:.4}px", space);
    println!("  'A': {:.4}px", a_upper);
    println!("  Sum: {:.4}px\n", space + a_upper);

    println!("Combined measurement:");
    println!("  ' A' (space + A): {:.4}px", space_a);
    println!("  Expected (no kerning): {:.4}px", space + a_upper);
    println!("  Kerning: {:.4}px\n", space_a - (space + a_upper));

    // Now test with Tab
    let tab_space = measure(&mut font_system, "Tab ");
    let tab_space_a = measure(&mut font_system, "Tab A");

    println!("With 'Tab' prefix:");
    println!("  'Tab ': {:.4}px", tab_space);
    println!("  'Tab A': {:.4}px", tab_space_a);
    println!("  'Tab A' - 'Tab ': {:.4}px", tab_space_a - tab_space);
    println!("  'A' alone: {:.4}px", a_upper);
    println!(
        "  Difference: {:.4}px\n",
        (tab_space_a - tab_space) - a_upper
    );

    // Test other letters after space
    println!("Testing space + other letters:");
    for letter in ['A', 'B', 'C', 'D', 'E'] {
        let letter_alone = measure(&mut font_system, &letter.to_string());
        let space_letter = measure(&mut font_system, &format!(" {}", letter));
        let kerning = space_letter - (space + letter_alone);
        println!(
            "  ' {}': {:.4}px (space: {:.4} + letter: {:.4} = {:.4}, kerning: {:.4}px)",
            letter,
            space_letter,
            space,
            letter_alone,
            space + letter_alone,
            kerning
        );
    }

    // Test with Tab prefix
    println!("\nTesting 'Tab ' + letter:");
    for letter in ['A', 'B', 'C', 'D', 'E'] {
        let tab_letter = measure(&mut font_system, &format!("Tab {}", letter));
        let letter_alone = measure(&mut font_system, &letter.to_string());
        let added_width = tab_letter - tab_space;
        let diff_from_letter = added_width - letter_alone;

        let chrome_expected = match letter {
            'A' => 38.4375,
            'B' => 38.4375,
            'C' => 38.4375,
            'D' => 39.3125,
            'E' => 37.5312,
            _ => 0.0,
        };

        let chrome_diff = tab_letter - chrome_expected;

        println!(
            "  'Tab {}': {:.4}px (Tab  + added: {:.4}px, letter: {:.4}px, diff: {:.4}px) [Chrome diff: {:.4}px]",
            letter, tab_letter, added_width, letter_alone, diff_from_letter, chrome_diff
        );
    }

    println!("\n=== KEY FINDING ===");
    println!("The width added when appending 'A' to 'Tab ':");
    println!("  Added width: {:.4}px", tab_space_a - tab_space);
    println!("  'A' width alone: {:.4}px", a_upper);
    println!(
        "  Extra width (kerning?): {:.4}px",
        (tab_space_a - tab_space) - a_upper
    );

    println!("\nChrome expects 'Tab A' to be: 38.4375px");
    println!("  Chrome's 'Tab ': 26.8828px (implied)");
    println!("  Chrome's added width for A: {:.4}px", 38.4375 - 26.8828);
    println!("  Our added width for A: {:.4}px", tab_space_a - tab_space);
    println!(
        "  Difference in added width: {:.4}px",
        (tab_space_a - tab_space) - (38.4375 - 26.8828)
    );
}
