use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    let mut font_system = FontSystem::new();
    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(400));

    let tests = vec![
        ("b A", "b with space then A"),
        ("bA", "b directly followed by A"),
        ("b D", "b with space then D"),
        ("bD", "b directly followed by D"),
    ];

    for (text, desc) in tests {
        let buffer_metrics = Metrics::new(16.0, 18.0);
        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        let width = buffer.line_layout(&mut font_system, 0).unwrap()[0].w;
        println!("'{}' ({}): width={:.4}px", text, desc, width);
    }
}
