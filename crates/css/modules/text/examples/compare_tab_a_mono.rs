use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn measure(font_system: &mut FontSystem, text: &str, family: &str) -> f32 {
    let attrs = Attrs::new()
        .family(Family::Name(family))
        .weight(Weight(400));
    let buffer_metrics = Metrics::new(16.0, 19.2);
    let mut buffer = Buffer::new(font_system, buffer_metrics);
    buffer.set_text(font_system, text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);

    if let Some(layout) = buffer.line_layout(font_system, 0) {
        let total: f32 = layout.iter().map(|l| l.w).sum();
        total
    } else {
        0.0
    }
}

fn main() {
    let mut font_system = FontSystem::new();

    let proportional_fonts = ["Liberation Serif", "DejaVu Serif", "Times New Roman"];
    let monospace_fonts = ["Liberation Mono", "DejaVu Sans Mono", "Courier New"];

    println!("PROPORTIONAL FONTS:");
    println!("===================\n");

    for font in &proportional_fonts {
        let tab_a = measure(&mut font_system, "Tab A", font);
        let tab_space = measure(&mut font_system, "Tab ", font);
        let a = measure(&mut font_system, "A", font);

        println!("{}", font);
        println!(
            "  Tab A     → {:.4}px (Chrome expects: 38.4375px, diff: {:+.4}px)",
            tab_a,
            tab_a - 38.4375
        );
        println!("  Tab       → {:.4}px", tab_space);
        println!("  A         → {:.4}px", a);
        println!("  Tab + A   → {:.4}px (sum)", tab_space + a);
        println!();
    }

    println!("\nMONOSPACE FONTS:");
    println!("================\n");

    for font in &monospace_fonts {
        let tab_a = measure(&mut font_system, "Tab A", font);
        let tab_space = measure(&mut font_system, "Tab ", font);
        let a = measure(&mut font_system, "A", font);

        println!("{}", font);
        println!("  Tab A     → {:.4}px", tab_a);
        println!("  Tab       → {:.4}px", tab_space);
        println!("  A         → {:.4}px", a);
        println!("  Tab + A   → {:.4}px (sum)", tab_space + a);
        println!();
    }
}
