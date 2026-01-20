use glyphon::{Attrs, Family, FontSystem};

fn main() {
    let mut font_system = FontSystem::new();
    font_system.db_mut().load_system_fonts();

    println!("=== Testing Monospace font matching ===\n");

    for weight in [400, 500, 600, 700] {
        let attrs = Attrs::new()
            .family(Family::Monospace)
            .weight(glyphon::Weight(weight));
        let matches = font_system.get_font_matches(&attrs);

        if let Some(first) = matches.first() {
            if let Some(face) = font_system.db().face(first.id) {
                println!("Requested weight: {} →", weight);
                println!("  Matched weight: {}", first.font_weight);
                println!("  Font family: {}", face.families[0].0);
                println!();
            }
        }
    }
}
