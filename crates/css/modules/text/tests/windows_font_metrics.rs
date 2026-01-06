//! Test to understand if Windows fonts need different rounding strategy

#[test]
fn analyze_consolas_metrics() {
    use glyphon::{Attrs, Family, FontSystem};

    let mut font_system = FontSystem::new();
    font_system.db_mut().load_system_fonts();

    // Test common Windows fonts
    let fonts_to_test = vec![
        ("Consolas", "Windows default monospace"),
        ("Courier New", "Windows fallback monospace"),
        ("DejaVu Sans Mono", "Linux default monospace"),
    ];

    for (font_name, desc) in fonts_to_test {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("{} - {}", font_name, desc);
        eprintln!("{}", "=".repeat(70));

        let attrs = Attrs::new().family(Family::Name(font_name));
        let font_matches = font_system.get_font_matches(&attrs);

        if font_matches.is_empty() {
            eprintln!("Font not available");
            continue;
        }

        let first_match = &font_matches[0];
        if let Some(font) = font_system.get_font(
            first_match.id,
            glyphon::fontdb::Weight(first_match.font_weight),
        ) {
            let metrics = font.metrics();
            let units_per_em = f32::from(metrics.units_per_em);
            let ascent_norm = metrics.ascent / units_per_em;
            let descent_norm = -metrics.descent / units_per_em;

            eprintln!("Normalized metrics:");
            eprintln!("  ascent:  {:.10}", ascent_norm);
            eprintln!("  descent: {:.10}", descent_norm);

            // Test at 16px
            let font_size = 16.0;
            let ascent_px = ascent_norm * font_size;
            let descent_px = descent_norm * font_size;
            let total = ascent_px + descent_px;

            eprintln!("\nAt 16px:");
            eprintln!("  ascent:  {:.10}", ascent_px);
            eprintln!("  descent: {:.10}", descent_px);
            eprintln!("  total:   {:.10}", total);

            let round_asc = ascent_px.round();
            let round_desc = descent_px.round();
            let round_total = round_asc + round_desc;

            let floor_total = total.floor();

            eprintln!("\nRounding strategies:");
            eprintln!(
                "  round(asc) + round(desc) = {} + {} = {}",
                round_asc, round_desc, round_total
            );
            eprintln!("  floor(total)             = {}", floor_total);

            if round_total == floor_total {
                eprintln!("  ✓ BOTH STRATEGIES AGREE");
            } else {
                eprintln!(
                    "  ✗ STRATEGIES DIFFER (round={}, floor={})",
                    round_total, floor_total
                );
            }
        }
    }
}
