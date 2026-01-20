use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    let mut font_system = FontSystem::new();
    let text = "Option 1 (checked)";

    println!("Testing width of: \"{}\" ({} chars)\n", text, text.len());

    for (name, family) in [
        ("Liberation Serif", Family::Name("Liberation Serif")),
        ("Liberation Mono", Family::Monospace),
    ] {
        let attrs = Attrs::new().family(family).weight(Weight(400));
        let metrics = Metrics::new(13.0, 15.0);
        let mut buffer = Buffer::new(&mut font_system, metrics);
        buffer.set_text(&mut font_system, text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);

        if let Some(layout) = buffer.line_layout(&mut font_system, 0) {
            let width: f32 = layout.iter().map(|l| l.w).sum();
            println!(
                "{:25} = {:.4}px ({:.2}px per char)",
                name,
                width,
                width / text.len() as f32
            );
        }
    }

    println!("\nExpected (Chrome): 140.890625px");
    println!("Actual (Valor):    70.4375px");
}
