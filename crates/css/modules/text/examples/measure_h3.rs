use glyphon::cosmic_text::Buffer;
use glyphon::cosmic_text::{Attrs, Family, FontSystem, Metrics, Weight};

fn main() {
    let mut font_system = FontSystem::new();

    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(700));

    // Get font metrics
    let font_matches = font_system.get_font_matches(&attrs);
    if font_matches.is_empty() {
        println!("No font matches found!");
        return;
    }

    let font_id = font_matches[0];
    if let Some(font) = font_system.get_font(font_id) {
        let metrics = font.as_swash().metrics(&[]);

        // At 18.72px
        let font_size = 18.72;
        let units_per_em = metrics.units_per_em as f32;
        let scale = font_size / units_per_em;

        let ascent = metrics.ascent * scale;
        let descent = -metrics.descent * scale; // descent is negative in font metrics
        let leading = metrics.leading * scale;

        println!("Liberation Serif Bold at 18.72px:");
        println!("  ascent: {:.4}px", ascent);
        println!("  descent: {:.4}px", descent);
        println!("  leading: {:.4}px", leading);
        println!("  total (a+d+l): {:.4}px", ascent + descent + leading);

        // Apply threshold rounding
        let total = ascent + descent + leading;
        let fract = total.fract();
        let line_height = if fract < 0.65 {
            total.floor()
        } else {
            total.ceil()
        };

        println!(
            "  line-height (0.65 threshold): {:.0}px (fract={:.4})",
            line_height, fract
        );

        // Chrome expects 22px for h3 height
        println!("\nChrome expects: 22px");
        println!("Difference: {}", line_height - 22.0);
    }
}
