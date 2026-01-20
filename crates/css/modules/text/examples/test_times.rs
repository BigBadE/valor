use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    let mut font_system = FontSystem::new();
    let buffer_metrics = Metrics::new(16.0, 18.0);

    println!("=== Testing different fonts ===");

    for font_name in ["Liberation Serif", "Times New Roman", "serif"] {
        let attrs = Attrs::new()
            .family(Family::Name(font_name))
            .weight(Weight(400));

        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, "Tab A", &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        let width = buffer.line_layout(&mut font_system, 0).unwrap()[0].w;

        println!("{:20} 'Tab A': {:.4}px", font_name, width);
    }

    println!("\nChrome expects: 38.4375px");
}
