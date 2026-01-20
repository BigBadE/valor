use glyphon::{Attrs, Family, FontSystem, fontdb::Weight};

fn main() {
    let mut font_system = FontSystem::new();
    font_system.db_mut().load_system_fonts();

    // Test monospace font with weight 400
    let attrs_regular = Attrs::new()
        .family(Family::Monospace)
        .weight(glyphon::Weight(400));
    let matches = font_system.get_font_matches(&attrs_regular);
    if let Some(first) = matches.first() {
        if let Some(font) = font_system.get_font(first.id, Weight(400)) {
            let metrics = font.metrics();
            let units_per_em = metrics.units_per_em as f32;
            let ascent = metrics.ascent / units_per_em;
            let descent = -metrics.descent / units_per_em;
            let total = ascent + descent;
            println!(
                "Monospace Regular (400): ascent={:.4}, descent={:.4}, total={:.4}",
                ascent, descent, total
            );
        }
    }

    // Test monospace font with weight 600
    let attrs_bold = Attrs::new()
        .family(Family::Monospace)
        .weight(glyphon::Weight(600));
    let matches = font_system.get_font_matches(&attrs_bold);
    if let Some(first) = matches.first() {
        let weight = Weight(first.font_weight);
        if let Some(font) = font_system.get_font(first.id, weight) {
            let metrics = font.metrics();
            let units_per_em = metrics.units_per_em as f32;
            let ascent = metrics.ascent / units_per_em;
            let descent = -metrics.descent / units_per_em;
            let total = ascent + descent;
            println!(
                "Monospace Bold (600->{}): ascent={:.4}, descent={:.4}, total={:.4}",
                first.font_weight, ascent, descent, total
            );
        }
    }
}
