//! Measure the exact text strings from failing tests

#[test]
fn measure_one_two_widths() {
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

    eprintln!("\n=== Measuring Text Widths ===");

    for text in ["one", "two", "o", "n", "e", "t", "w"] {
        let metrics = Metrics::new(16.0, 19.0);
        let mut buffer = Buffer::new(&mut font_system, metrics);

        buffer.set_text(&mut font_system, text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);

        if let Some(layout_lines) = buffer.line_layout(&mut font_system, 0) {
            for layout_line in layout_lines {
                eprintln!("{:5} width: {:.10}", text, layout_line.w);

                // Also check individual glyphs
                if text.len() <= 3 {
                    for glyph in &layout_line.glyphs {
                        eprintln!("    glyph: x={:.6} w={:.6}", glyph.x, glyph.w);
                    }
                }
            }
        }
    }

    eprintln!("\nChrome expects:");
    eprintln!("  'one': 28.8125px");
    eprintln!("  'two': 28.8125px");
    eprintln!("\nSingle char from earlier test:");
    eprintln!("  Chrome: 9.609375px");
    eprintln!("  Valor:  9.632812px");
}
