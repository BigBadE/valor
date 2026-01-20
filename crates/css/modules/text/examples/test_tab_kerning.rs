use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    let mut font_system = FontSystem::new();
    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(400));
    
    // Measure "Tab" prefix
    let tab_width = {
        let buffer_metrics = Metrics::new(16.0, 18.0);
        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, "Tab ", &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        buffer.line_layout(&mut font_system, 0).unwrap()[0].w
    };
    
    println!("'Tab ': width={:.4}px", tab_width);
    
    // Measure each full string
    for letter in &["A", "D"] {
        let full_text = format!("Tab {}", letter);
        let buffer_metrics = Metrics::new(16.0, 18.0);
        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, &full_text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        let width = buffer.line_layout(&mut font_system, 0).unwrap()[0].w;
        
        // Also measure just the letter
        let buffer_metrics2 = Metrics::new(16.0, 18.0);
        let mut buffer2 = Buffer::new(&mut font_system, buffer_metrics2);
        buffer2.set_text(&mut font_system, letter, &attrs, Shaping::Advanced, None);
        buffer2.shape_until_scroll(&mut font_system, false);
        let letter_width = buffer2.line_layout(&mut font_system, 0).unwrap()[0].w;
        
        let expected = tab_width + letter_width;
        let kerning = width - expected;
        
        println!("'{}': total={:.4}px, expected={:.4}px (Tab + letter), kerning={:.4}px", 
            full_text, width, expected, kerning);
    }
}
