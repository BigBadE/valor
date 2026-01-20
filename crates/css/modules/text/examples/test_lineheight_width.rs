use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    let mut font_system = FontSystem::new();
    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(400));
    
    // Test with different line heights
    let line_heights = vec![16.0, 18.0, 20.0, 22.0, 24.0];
    
    for lh in line_heights {
        let buffer_metrics = Metrics::new(16.0, lh);
        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, "Tab A", &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        
        for line_idx in 0..buffer.lines.len() {
            if let Some(layout_lines) = buffer.line_layout(&mut font_system, line_idx) {
                for layout_line in layout_lines {
                    println!("line_height={:.1} -> width: {:.4}px", lh, layout_line.w);
                }
            }
        }
    }
}
