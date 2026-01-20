use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    let mut font_system = FontSystem::new();
    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(400));
    let buffer_metrics = Metrics::new(16.0, 18.0);

    let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
    buffer.set_text(&mut font_system, "Tab A", &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(&mut font_system, false);

    let width = buffer.line_layout(&mut font_system, 0).unwrap()[0].w;
    println!("=== Final measurement ===");
    println!("'Tab A' at font-size 16px: {:.4}px", width);
    println!("Chrome expects: 38.4375px");
    println!("Difference: {:.4}px", width - 38.4375);
    println!("\nRatio: {:.6}", 38.4375 / width);
    println!(
        "If we scaled font-size by this ratio: {:.4}px",
        16.0 * (38.4375 / width)
    );
}
