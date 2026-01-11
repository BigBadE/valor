use css_text::measurement::font_system::{get_font_system, get_font_metrics};
use glyphon::{Attrs, Family};

fn main() {
    let font_system_arc = get_font_system();
    let mut font_system = font_system_arc.lock().unwrap();
    
    let attrs = Attrs::new().family(Family::Name("Liberation Sans"));
    
    if let Some(metrics) = get_font_metrics(&mut font_system, &attrs) {
        println!("Liberation Sans metrics:");
        println!("  ascent: {}", metrics.ascent);
        println!("  descent: {}", metrics.descent);
        println!("  leading: {}", metrics.leading);
        println!("  total (ascent + descent + leading): {}", metrics.ascent + metrics.descent + metrics.leading);
        
        // Calculate line-height for 14px font
        let font_size = 14.0;
        let ascent_px = metrics.ascent * font_size;
        let descent_px = metrics.descent * font_size;
        let leading_px = metrics.leading * font_size;
        let line_height = (ascent_px + descent_px + leading_px).round();
        
        println!("\nFor 14px font:");
        println!("  ascent: {}px", ascent_px);
        println!("  descent: {}px", descent_px);
        println!("  leading: {}px", leading_px);
        println!("  line-height (rounded): {}px", line_height);
    } else {
        println!("Failed to get metrics for Liberation Sans");
    }
}
