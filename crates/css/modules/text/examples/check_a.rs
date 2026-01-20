use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    let mut font_system = FontSystem::new();
    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(400));
    let buffer_metrics = Metrics::new(16.0, 18.0);
    
    for text in ["Tab ", "A", "Tab A"] {
        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        let width = buffer.line_layout(&mut font_system, 0).unwrap()[0].w;
        println!("'{}': {:.4}px", text, width);
    }
    
    println!("\nExpected:");
    println!("'Tab ': 26.8828px (Chrome)");
    println!("'A': ???");
    println!("'Tab A': 38.4375px (Chrome)");
    
    println!("\nIf 'Tab A' = 'Tab ' + 'A' with no kerning:");
    println!("Then 'A' = 38.4375 - 26.8828 = {:.4}px", 38.4375 - 26.8828);
    println!("We measure 'Tab A' - 'Tab ' = {:.4}px", 39.3125 - 27.7578);
}
