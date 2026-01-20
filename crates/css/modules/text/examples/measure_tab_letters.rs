use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    let mut font_system = FontSystem::new();
    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(400));
    
    let texts = vec!["Tab A", "Tab B", "Tab C", "Tab D", "Tab E"];
    
    for text in texts {
        let buffer_metrics = Metrics::new(16.0, 18.0);
        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        
        for line_idx in 0..buffer.lines.len() {
            if let Some(layout_lines) = buffer.line_layout(&mut font_system, line_idx) {
                for layout_line in layout_lines {
                    println!("'{}': width={:.4}px", text, layout_line.w);
                }
            }
        }
    }
    
    println!("\nChrome expects:");
    println!("Tab A: 38.4375px");
    println!("Tab B: 38.4375px");  
    println!("Tab C: 38.4375px");
    println!("Tab D: 39.3125px");
    println!("Tab E: 37.53125px");
}
