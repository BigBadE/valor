//! Deep investigation of cosmic-text glyph advances

#[test]
fn investigate_glyph_advances() {
    use glyphon::{Attrs, Family, FontSystem, Metrics, Buffer, Shaping, Weight};

    let mut font_system = FontSystem::new();
    font_system.db_mut().load_system_fonts();

    #[cfg(all(unix, not(target_os = "macos")))]
    font_system.db_mut().set_monospace_family("DejaVu Sans Mono");

    let attrs = Attrs::new()
        .family(Family::Name("DejaVu Sans Mono"))
        .weight(Weight(400));

    eprintln!("\n=== INVESTIGATING COSMIC-TEXT GLYPH ADVANCES ===\n");

    // Get font metrics
    let font_matches = font_system.get_font_matches(&attrs);
    if let Some(first_match) = font_matches.first() {
        eprintln!("Font matched: weight={}", first_match.font_weight);

        if let Some(font) = font_system.get_font(first_match.id, glyphon::fontdb::Weight(first_match.font_weight)) {
            let metrics = font.metrics();
            eprintln!("Font metrics:");
            eprintln!("  units_per_em: {}", metrics.units_per_em);
            eprintln!("  ascent: {}", metrics.ascent);
            eprintln!("  descent: {}", metrics.descent);
        }
    }

    let metrics = Metrics::new(16.0, 19.0);
    let mut buffer = Buffer::new(&mut font_system, metrics);

    eprintln!("\n=== Testing 'one' ===");
    buffer.set_text(&mut font_system, "one", &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(&mut font_system, false);

    if let Some(layout_lines) = buffer.line_layout(&mut font_system, 0) {
        for layout_line in layout_lines {
            eprintln!("\nLayout line total width: {:.10}", layout_line.w);
            eprintln!("Number of glyphs: {}", layout_line.glyphs.len());

            for (i, glyph) in layout_line.glyphs.iter().enumerate() {
                eprintln!("\nGlyph {}:", i);
                eprintln!("  glyph_id: {}", glyph.glyph_id);
                eprintln!("  x: {:.10}", glyph.x);
                eprintln!("  w: {:.10}", glyph.w);
                eprintln!("  w * 64: {:.10}", glyph.w * 64.0);

                let w_in_freetype = glyph.w * 64.0;
                eprintln!("  FreeType units: {:.2}", w_in_freetype);
                eprintln!("  floor(FT): {}", w_in_freetype.floor());
                eprintln!("  round(FT): {}", w_in_freetype.round());
            }

            let sum_widths: f32 = layout_line.glyphs.iter().map(|g| g.w).sum();
            eprintln!("\nSum of glyph widths: {:.10}", sum_widths);
            eprintln!("Difference from line.w: {:.10}", (layout_line.w - sum_widths).abs());
        }
    }

    eprintln!("\n=== Testing individual characters ===");
    for ch in ['o', 'n', 'e'] {
        let mut buffer = Buffer::new(&mut font_system, metrics);
        buffer.set_text(&mut font_system, &ch.to_string(), &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);

        if let Some(layout_lines) = buffer.line_layout(&mut font_system, 0) {
            for layout_line in layout_lines {
                for glyph in &layout_line.glyphs {
                    eprintln!("\n'{}': w={:.10}, w*64={:.2}, glyph_id={}",
                        ch, glyph.w, glyph.w * 64.0, glyph.glyph_id);
                }
            }
        }
    }

    eprintln!("\n=== Chrome Expectations ===");
    eprintln!("'one' total: 28.8125 = 1844 / 64");
    eprintln!("per char: 9.609375 = 615 / 64");
    eprintln!("\nIf cosmic-text returns 616.5 FT units:");
    eprintln!("  616.5 / 64 = 9.6328125");
    eprintln!("  That's +1.5 FT units from Chrome's 615");
}
