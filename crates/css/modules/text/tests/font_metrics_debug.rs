#[test]
fn debug_dejavu_font_metrics() {
    use glyphon::{Attrs, Family, FontSystem, fontdb};

    let mut font_system = FontSystem::new();
    font_system.db_mut().load_system_fonts();
    font_system
        .db_mut()
        .set_monospace_family("DejaVu Sans Mono");

    let attrs = Attrs::new().family(Family::Name("DejaVu Sans Mono"));
    let font_matches = font_system.get_font_matches(&attrs);

    if let Some(first_match) = font_matches.first() {
        eprintln!(
            "Font matched: ID={:?}, Weight={}",
            first_match.id, first_match.font_weight
        );

        if let Some(font) =
            font_system.get_font(first_match.id, fontdb::Weight(first_match.font_weight))
        {
            let metrics = font.metrics();
            let units_per_em = f32::from(metrics.units_per_em);

            eprintln!("\nhhea metrics:");
            eprintln!(
                "  ascent: {} ({:.6})",
                metrics.ascent,
                metrics.ascent / units_per_em
            );
            eprintln!(
                "  descent: {} ({:.6})",
                metrics.descent,
                -metrics.descent / units_per_em
            );
            eprintln!(
                "  leading: {} ({:.6})",
                metrics.leading,
                metrics.leading / units_per_em
            );
            eprintln!("  units_per_em: {}", units_per_em);

            if let Some((win_ascent, win_descent)) = font.os2_metrics() {
                eprintln!("\nOS/2 metrics (AVAILABLE):");
                eprintln!("  winAscent: {:.6}", win_ascent);
                eprintln!("  winDescent: {:.6}", win_descent);
                eprintln!("  total: {:.6}", win_ascent + win_descent);
            } else {
                eprintln!("\nOS/2 metrics: NOT AVAILABLE");
            }

            // Calculate what Valor uses on Linux (16px font)
            let font_size = 16.0;
            let ascent_linux = (metrics.ascent / units_per_em) * font_size;
            let descent_linux = (-metrics.descent / units_per_em) * font_size;
            let leading_linux = (metrics.leading / units_per_em) * font_size;

            eprintln!("\nValor Linux calculation @ 16px (hhea):");
            eprintln!("  ascent: {:.6}", ascent_linux);
            eprintln!("  descent: {:.6}", descent_linux);
            eprintln!("  leading: {:.6}", leading_linux);
            eprintln!("  glyph_height: {:.6}", ascent_linux + descent_linux);
            eprintln!(
                "  line_height: {:.6}",
                ascent_linux + descent_linux + leading_linux
            );

            if let Some((win_ascent, win_descent)) = font.os2_metrics() {
                let win_asc_px = win_ascent * font_size;
                let win_desc_px = win_descent * font_size;
                eprintln!("\nValor Windows calculation @ 16px (OS/2):");
                eprintln!("  ascent: {:.6}", win_asc_px);
                eprintln!("  descent: {:.6}", win_desc_px);
                eprintln!("  glyph_height: {:.6}", win_asc_px + win_desc_px);
                eprintln!("  line_height: {:.6} (leading=0)", win_asc_px + win_desc_px);
            }

            // Test actual glyph advances for A, B, C
            eprintln!("\nGlyph advances @ 16px:");
            for ch in ['A', 'B', 'C'] {
                use glyphon::{Buffer, Metrics, Shaping};
                let test_attrs = Attrs::new()
                    .family(Family::Name("DejaVu Sans Mono"))
                    .weight(glyphon::Weight(400));
                let mut buffer = Buffer::new(&mut font_system, Metrics::new(16.0, 16.0));
                buffer.set_text(
                    &mut font_system,
                    &ch.to_string(),
                    &test_attrs,
                    Shaping::Advanced,
                    None,
                );
                buffer.shape_until_scroll(&mut font_system, false);

                if let Some(layout_lines) = buffer.line_layout(&mut font_system, 0) {
                    for layout_line in layout_lines {
                        eprintln!("  '{}': width={:.6}", ch, layout_line.w);
                    }
                }
            }
        }
    }
}
