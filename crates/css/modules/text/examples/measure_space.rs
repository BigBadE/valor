use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    let mut font_system = FontSystem::new();
    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(400));
    let buffer_metrics = Metrics::new(16.0, 18.0);
    
    // Measure individual characters
    let texts = vec!["T", "a", "b", " ", "A", "Tab", " A", "Tab A"];
    
    for text in texts {
        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        
        for line_idx in 0..buffer.lines.len() {
            if let Some(layout_lines) = buffer.line_layout(&mut font_system, line_idx) {
                for layout_line in layout_lines {
                    println!("'{}' width: {:.4}px", text, layout_line.w);
                }
            }
        }
    }
}
