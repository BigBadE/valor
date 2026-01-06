//! Compare font metrics between Valor and Chrome's behavior.
//!
//! This test examines the exact font metrics and rounding strategies
//! to understand how Chrome handles DejaVu Sans Mono on Linux.

#[test]
fn compare_dejavu_metrics_with_chrome() {
    use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

    let mut font_system = FontSystem::new();
    font_system.db_mut().load_system_fonts();
    font_system
        .db_mut()
        .set_monospace_family("DejaVu Sans Mono");

    let attrs = Attrs::new()
        .family(Family::Name("DejaVu Sans Mono"))
        .weight(glyphon::Weight(400));

    let font_matches = font_system.get_font_matches(&attrs);

    if let Some(first_match) = font_matches.first() {
        if let Some(font) = font_system.get_font(
            first_match.id,
            glyphon::fontdb::Weight(first_match.font_weight),
        ) {
            let metrics = font.metrics();
            let units_per_em = f32::from(metrics.units_per_em);

            // Raw metrics (normalized to 1.0 scale)
            let ascent_norm = metrics.ascent / units_per_em;
            let descent_norm = -metrics.descent / units_per_em;
            let leading_norm = metrics.leading / units_per_em;

            eprintln!("\n=== DejaVu Sans Mono Font Metrics (normalized) ===");
            eprintln!(
                "Ascent:  {:.10} ({}/{})",
                ascent_norm, metrics.ascent, units_per_em
            );
            eprintln!(
                "Descent: {:.10} ({}/{})",
                descent_norm, -metrics.descent, units_per_em
            );
            eprintln!(
                "Leading: {:.10} ({}/{})",
                leading_norm, metrics.leading, units_per_em
            );

            // Test at different font sizes
            for font_size in [12.0, 14.0, 16.0, 18.0, 20.0, 24.0] {
                eprintln!("\n--- Font size: {}px ---", font_size);

                let ascent_px = ascent_norm * font_size;
                let descent_px = descent_norm * font_size;
                let leading_px = leading_norm * font_size;
                let glyph_h = ascent_px + descent_px;

                eprintln!("Unrounded:");
                eprintln!("  ascent:  {:.10}", ascent_px);
                eprintln!("  descent: {:.10}", descent_px);
                eprintln!("  leading: {:.10}", leading_px);
                eprintln!("  glyph_h: {:.10}", glyph_h);
                eprintln!("  line_h:  {:.10}", glyph_h + leading_px);

                // Strategy 1: Round components individually (current Valor)
                let s1_asc = ascent_px.round();
                let s1_desc = descent_px.round();
                let s1_lead = leading_px.round();
                let s1_glyph = s1_asc + s1_desc;
                let s1_line = s1_glyph + s1_lead;

                eprintln!("Strategy 1 (round components):");
                eprintln!("  ascent:  {} (from {:.10})", s1_asc, ascent_px);
                eprintln!("  descent: {} (from {:.10})", s1_desc, descent_px);
                eprintln!("  glyph_h: {} ({}+{})", s1_glyph, s1_asc, s1_desc);
                eprintln!("  line_h:  {}", s1_line);

                // Strategy 2: Floor total glyph height
                let s2_glyph = glyph_h.floor();
                let s2_lead = leading_px.round();
                let s2_line = s2_glyph + s2_lead;

                eprintln!("Strategy 2 (floor total):");
                eprintln!("  glyph_h: {} (floor of {:.10})", s2_glyph, glyph_h);
                eprintln!("  line_h:  {}", s2_line);

                // Strategy 3: Round total glyph height
                let s3_glyph = glyph_h.round();
                let s3_lead = leading_px.round();
                let s3_line = s3_glyph + s3_lead;

                eprintln!("Strategy 3 (round total):");
                eprintln!("  glyph_h: {} (round of {:.10})", s3_glyph, glyph_h);
                eprintln!("  line_h:  {}", s3_line);

                // Strategy 4: Ceil components
                let s4_asc = ascent_px.ceil();
                let s4_desc = descent_px.ceil();
                let s4_glyph = s4_asc + s4_desc;

                eprintln!("Strategy 4 (ceil components):");
                eprintln!("  ascent:  {} (ceil of {:.10})", s4_asc, ascent_px);
                eprintln!("  descent: {} (ceil of {:.10})", s4_desc, descent_px);
                eprintln!("  glyph_h: {}", s4_glyph);

                // Test actual text rendering
                let mut buffer = Buffer::new(&mut font_system, Metrics::new(font_size, s1_line));
                buffer.set_text(&mut font_system, "Test", &attrs, Shaping::Advanced, None);
                buffer.shape_until_scroll(&mut font_system, false);

                if let Some(layout_lines) = buffer.line_layout(&mut font_system, 0) {
                    for layout_line in layout_lines {
                        eprintln!("Actual cosmic-text layout:");
                        eprintln!("  max_ascent:  {:.10}", layout_line.max_ascent);
                        eprintln!("  max_descent: {:.10}", layout_line.max_descent);
                        eprintln!(
                            "  total:       {:.10}",
                            layout_line.max_ascent + layout_line.max_descent
                        );
                        eprintln!("  line_height_opt: {:?}", layout_line.line_height_opt);
                    }
                }
            }
        }
    }
}

#[test]
fn test_chrome_known_values() {
    // Based on actual Chrome measurements on Linux with DejaVu Sans Mono
    // These are the values Chrome reports for specific test cases

    eprintln!("\n=== Chrome Known Values (from actual tests) ===");
    eprintln!("Font: DejaVu Sans Mono, 16px");
    eprintln!("Chrome reports: height=18px (expected)");
    eprintln!("Valor produces: height=19px (current)");
    eprintln!("\nThe question: How does Chrome get 18 from these metrics?");
    eprintln!("  ascent:  14.851562 → round = 15");
    eprintln!("  descent:  3.773438 → round = 4");
    eprintln!("  15 + 4 = 19 (Valor's calculation)");
    eprintln!("  But Chrome expects 18");
    eprintln!("\nPossible explanations:");
    eprintln!("  1. Chrome uses floor(18.625) = 18");
    eprintln!("  2. Chrome uses different ascent/descent values");
    eprintln!("  3. Chrome uses hhea vs OS/2 tables differently");
    eprintln!("  4. Chrome applies a different rounding strategy");
}
