//! Extract raw glyph advance from font file

#[test]
fn extract_raw_glyph_advance() {
    use glyphon::{Attrs, Family, FontSystem, Weight};

    let mut font_system = FontSystem::new();
    font_system.db_mut().load_system_fonts();

    #[cfg(all(unix, not(target_os = "macos")))]
    font_system
        .db_mut()
        .set_monospace_family("DejaVu Sans Mono");

    let attrs = Attrs::new()
        .family(Family::Name("DejaVu Sans Mono"))
        .weight(Weight(400));

    let font_matches = font_system.get_font_matches(&attrs);

    eprintln!("\n=== EXTRACTING RAW GLYPH ADVANCES ===\n");

    if let Some(first_match) = font_matches.first() {
        if let Some(font) = font_system.get_font(
            first_match.id,
            glyphon::fontdb::Weight(first_match.font_weight),
        ) {
            let metrics = font.metrics();
            let units_per_em = metrics.units_per_em;

            eprintln!("units_per_em: {}", units_per_em);

            // Try to get raw advance for glyph IDs we know: 82=o, 81=n, 72=e
            for (ch, glyph_id) in [('o', 82u16), ('n', 81u16), ('e', 72u16)] {
                eprintln!("\n'{}' (glyph_id {}):", ch, glyph_id);

                // Try to access the font's glyph data directly
                // This might not be exposed through the current API
                // Let me check what methods are available

                // Calculate what the advance should be based on observed values
                // cosmic-text returns 616.5 FT units at 16px
                // 616.5 FT units = 616.5 / 64 px = 9.6328125px
                // In font units: 9.6328125 * units_per_em / 16
                let observed_px = 9.6328125;
                let calculated_font_units = observed_px * (units_per_em as f32) / 16.0;

                eprintln!("  Observed: 616.5 FT units = {}px", observed_px);
                eprintln!("  Calculated font units: {:.2}", calculated_font_units);
                eprintln!("  If exact: {}", calculated_font_units as i32);

                // Chrome expects 615 FT units
                let chrome_px = 9.609375;
                let chrome_font_units = chrome_px * (units_per_em as f32) / 16.0;
                eprintln!("  Chrome: 615 FT units = {}px", chrome_px);
                eprintln!("  Chrome font units: {:.2}", chrome_font_units);
                eprintln!("  If exact: {}", chrome_font_units as i32);
            }

            eprintln!("\n=== Analysis ===");
            eprintln!("cosmic-text (skrifa): 1233 font units → 616.5 FT units → 9.6328125px");
            eprintln!("Chrome (FreeType):    1230 font units → 615.0 FT units → 9.609375px");
            eprintln!("Difference: 3 font units = 1.5 FT units = 0.0234375px");
            eprintln!("\nThis suggests the raw font value is either:");
            eprintln!("  - 1233 (skrifa is exact, FreeType is rounding down by 3)");
            eprintln!("  - 1230 (FreeType is exact, skrifa is rounding up by 3)");
            eprintln!("  - Something in between (both are rounding differently)");
        }
    }
}
