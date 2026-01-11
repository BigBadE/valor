use glyphon::{Attrs, Family, FontSystem, Weight};

fn main() {
    let mut font_system = FontSystem::new();
    font_system.db_mut().load_system_fonts();

    // Test Noto Sans - what Chrome actually uses for system-ui!
    let attrs = Attrs::new()
        .family(Family::Name("Noto Sans"))
        .weight(Weight(400));

    let font_matches = font_system.get_font_matches(&attrs);
    if let Some(first_match) = font_matches.first() {
        let font = font_system
            .get_font(
                first_match.id,
                glyphon::fontdb::Weight(first_match.font_weight),
            )
            .unwrap();

        println!("=== Noto Sans Metrics (Chrome's actual system-ui font) ===");
        println!();

        let metrics = font.metrics();
        let upem = f32::from(metrics.units_per_em);

        println!("hhea metrics:");
        println!("  ascent: {}", metrics.ascent);
        println!("  descent: {}", metrics.descent);
        println!("  leading: {}", metrics.leading);
        println!("  units_per_em: {}", metrics.units_per_em);
        println!();

        // Get OS/2 typo metrics
        if let Some((typo_ascent, typo_descent, typo_line_gap, use_typo_metrics)) =
            font.os2_typo_metrics()
        {
            println!("OS/2 typo metrics:");
            println!(
                "  sTypoAscender: {} (normalized: {})",
                (typo_ascent * upem) as i16,
                typo_ascent
            );
            println!(
                "  sTypoDescender: {} (normalized: {})",
                -(typo_descent * upem) as i16,
                typo_descent
            );
            println!(
                "  sTypoLineGap: {} (normalized: {})",
                (typo_line_gap * upem) as i16,
                typo_line_gap
            );
            println!("  USE_TYPO_METRICS flag (bit 7): {}", use_typo_metrics);
            println!();

            let font_size = 14.0;
            let ascent_px = typo_ascent * font_size;
            let descent_px = typo_descent * font_size;
            let leading_px = typo_line_gap * font_size;
            let total_px = ascent_px + descent_px + leading_px;
            println!("  For 14px font (typo metrics):");
            println!("    ascent: {:.2}px", ascent_px);
            println!("    descent: {:.2}px", descent_px);
            println!("    line_gap: {:.2}px", leading_px);
            println!("    total: {:.2}px", total_px);
            println!();
        }

        // Get OS/2 win metrics
        if let Some((win_ascent, win_descent)) = font.os2_metrics() {
            println!("OS/2 win metrics:");
            println!(
                "  usWinAscent: {} (normalized: {})",
                (win_ascent * upem) as u16,
                win_ascent
            );
            println!(
                "  usWinDescent: {} (normalized: {})",
                (win_descent * upem) as u16,
                win_descent
            );
            println!();

            let font_size = 14.0;
            let ascent_px = win_ascent * font_size;
            let descent_px = win_descent * font_size;
            let total_px = ascent_px + descent_px;
            println!("  For 14px font (win metrics):");
            println!("    ascent: {:.2}px", ascent_px);
            println!("    descent: {:.2}px", descent_px);
            println!("    total: {:.2}px", total_px);
            println!();
        }

        // Calculate hhea for 14px
        let font_size = 14.0;
        let ascent_px = (metrics.ascent / upem) * font_size;
        let descent_px = (-metrics.descent / upem) * font_size;
        let leading_px = (metrics.leading / upem) * font_size;
        let total_px = ascent_px + descent_px + leading_px;
        println!("For 14px font (hhea metrics):");
        println!("  ascent: {:.2}px", ascent_px);
        println!("  descent: {:.2}px", descent_px);
        println!("  line_gap: {:.2}px", leading_px);
        println!("  total: {:.2}px", total_px);
        println!();

        println!("=== Chrome Reported ===");
        println!("fontBoundingBoxAscent: 15px");
        println!("fontBoundingBoxDescent: 4px");
        println!("Total: 19px");
    } else {
        println!("Noto Sans not found!");
    }
}
