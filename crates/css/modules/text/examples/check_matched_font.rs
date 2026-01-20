use glyphon::{Attrs, Family, FontSystem};

fn main() {
    let mut font_system = FontSystem::new();
    font_system.db_mut().load_system_fonts();

    // Test monospace weight 600
    let attrs = Attrs::new()
        .family(Family::Monospace)
        .weight(glyphon::Weight(600));
    let matches = font_system.get_font_matches(&attrs);

    if let Some(first) = matches.first() {
        println!("Requested: Monospace, weight=600");
        println!("Matched weight: {}", first.font_weight);

        // Get font face info
        if let Some(face) = font_system.db().face(first.id) {
            println!("Font family: {}", face.families[0].0);
            println!("Font style: {:?}", face.style);
            println!("Font weight: {:?}", face.weight);
            println!("Font stretch: {:?}", face.stretch);
        }
    }
}
