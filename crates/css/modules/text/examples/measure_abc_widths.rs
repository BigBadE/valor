use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    let mut font_system = FontSystem::new();
    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(400));
    
    println!("Individual letter widths:");
    for letter in &["A", "B", "C", "D", "E"] {
        let buffer_metrics = Metrics::new(16.0, 18.0);
        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, letter, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        let width = buffer.line_layout(&mut font_system, 0).unwrap()[0].w;
        println!("  {}: {:.4}px", letter, width);
    }
    
    println!("\nTab + letter widths:");
    for letter in &["A", "B", "C", "D", "E"] {
        let text = format!("Tab {}", letter);
        let buffer_metrics = Metrics::new(16.0, 18.0);
        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, &text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        let width = buffer.line_layout(&mut font_system, 0).unwrap()[0].w;
        println!("  Tab {}: {:.4}px", letter, width);
    }
    
    println!("\nChrome expects:");
    println!("  Tab A/B/C: 38.4375px");
    println!("  Tab D: 39.3125px");
    println!("  Tab E: 37.53125px");
}
