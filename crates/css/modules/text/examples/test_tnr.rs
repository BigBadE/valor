use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    let mut font_system = FontSystem::new();
    let buffer_metrics = Metrics::new(16.0, 18.0);

    println!("=== Testing Times New Roman vs Liberation Serif ===\n");

    // Test with explicit font names
    for font_name in ["Times New Roman", "Liberation Serif"] {
        let attrs = Attrs::new()
            .family(Family::Name(font_name))
            .weight(Weight(400));

        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, "Tab A", &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        let width = buffer.line_layout(&mut font_system, 0).unwrap()[0].w;

        println!("{:20} → 'Tab A': {:.4}px", font_name, width);
    }

    // Also test with Family::Serif
    let attrs = Attrs::new().family(Family::Serif).weight(Weight(400));

    let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
    buffer.set_text(&mut font_system, "Tab A", &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(&mut font_system, false);
    let width = buffer.line_layout(&mut font_system, 0).unwrap()[0].w;

    println!("{:20} → 'Tab A': {:.4}px", "Family::Serif", width);

    println!("\nChrome (Times New Roman at 16px): 38.4375px");
    println!("Our Liberation Serif: 39.3125px (diff: +0.8750px)");
}
