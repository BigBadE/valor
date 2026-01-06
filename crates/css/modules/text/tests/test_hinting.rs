//! Test different hinting modes

#[test]
fn compare_hinting_modes() {
    use glyphon::cosmic_text::Hinting;
    use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

    let mut font_system = FontSystem::new();
    font_system.db_mut().load_system_fonts();

    #[cfg(all(unix, not(target_os = "macos")))]
    font_system
        .db_mut()
        .set_monospace_family("DejaVu Sans Mono");

    let attrs = Attrs::new()
        .family(Family::Name("DejaVu Sans Mono"))
        .weight(glyphon::Weight(400));

    eprintln!("\n=== Testing Hinting Modes ===");

    for (name, hinting) in [
        ("Disabled (default)", Hinting::Disabled),
        ("Enabled", Hinting::Enabled),
    ] {
        let metrics = Metrics::new(16.0, 19.0);
        let mut buffer = Buffer::new(&mut font_system, metrics);
        buffer.set_hinting(&mut font_system, hinting);

        buffer.set_text(&mut font_system, "one", &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);

        if let Some(layout_lines) = buffer.line_layout(&mut font_system, 0) {
            for layout_line in layout_lines {
                eprintln!("\n{:20} width: {:.10}", name, layout_line.w);

                for (i, glyph) in layout_line.glyphs.iter().enumerate() {
                    eprintln!("  glyph[{}]: x={:.10} w={:.10}", i, glyph.x, glyph.w);
                }
            }
        }
    }

    eprintln!("\nChrome expects: 28.8125px");
    eprintln!("Possible values:");
    eprintln!("  floor: 28.8125 = 1844 / 64");
    eprintln!("  round: 28.8984375 = 1849.5 / 64");
}
