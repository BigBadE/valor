use css_text::measurement::font_system::{get_font_system, map_font_family};
use glyphon::Attrs;

fn main() {
    let font_sys = get_font_system();
    let mut font_sys = font_sys.lock().unwrap();

    // Test monospace weight 600 (the problematic case)
    let family = map_font_family("monospace");
    let attrs = Attrs::new().family(family).weight(glyphon::Weight(600));
    let matches = font_sys.get_font_matches(&attrs);

    if let Some(first) = matches.first() {
        println!("Requested: monospace, weight=600");
        println!("Matched weight: {}", first.font_weight);

        if let Some(face) = font_sys.db().face(first.id) {
            println!("Font family: {}", face.families[0].0);

            // Get actual metrics
            if let Some(font) =
                font_sys.get_font(first.id, glyphon::fontdb::Weight(first.font_weight))
            {
                let metrics = font.metrics();
                let units_per_em = metrics.units_per_em as f32;
                let hhea_total = (metrics.ascent - metrics.descent) / units_per_em;
                println!(
                    "hhea metrics: ascent={:.4}, descent={:.4}, total={:.4}",
                    metrics.ascent / units_per_em,
                    -metrics.descent / units_per_em,
                    hhea_total
                );
                println!("At 13px font-size: {:.2}px", hhea_total * 13.0);

                if hhea_total * 13.0 >= 15.0 && hhea_total * 13.0 <= 15.5 {
                    println!("✓ CORRECT: Height matches Chrome's expected 15px");
                } else {
                    println!("✗ WRONG: Height should be ~15px for Chrome compatibility");
                }
            }
        }
    }
}
