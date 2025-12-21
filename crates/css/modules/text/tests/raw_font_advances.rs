//! Get RAW font advances from the font file

#[test]
fn check_raw_font_advances() {
    use glyphon::{Attrs, Family, FontSystem};

    let mut font_system = FontSystem::new();
    font_system.db_mut().load_system_fonts();

    #[cfg(all(unix, not(target_os = "macos")))]
    font_system.db_mut().set_monospace_family("DejaVu Sans Mono");

    let attrs = Attrs::new()
        .family(Family::Name("DejaVu Sans Mono"))
        .weight(glyphon::Weight(400));

    let font_matches = font_system.get_font_matches(&attrs);

    if let Some(first_match) = font_matches.first() {
        if let Some(font) = font_system.get_font(first_match.id, glyphon::fontdb::Weight(first_match.font_weight)) {
            eprintln!("\n=== RAW Font Advances ===");

            // Get the font's units_per_em
            let metrics = font.metrics();
            let units_per_em = f32::from(metrics.units_per_em);
            eprintln!("units_per_em: {}", units_per_em);

            // Try to get raw glyph data for 'o', 'n', 'e'
            for ch in ['o', 'n', 'e', 't', 'w'] {
                // cosmic-text/glyphon might not expose raw advances directly
                // We need to check the shaped output
                eprintln!("\nCharacter: '{}'", ch);

                // The glyph ID would be needed to get raw advances
                // For now, let's see what cosmic-text's shaping gives us
            }

            eprintln!("\n16px scaling:");
            eprintln!("  If raw advance = 615 units:");
            eprintln!("    615 / {} × 16 = {:.10}", units_per_em, 615.0 / units_per_em * 16.0);
            eprintln!("  If raw advance = 616 units:");
            eprintln!("    616 / {} × 16 = {:.10}", units_per_em, 616.0 / units_per_em * 16.0);

            // Check what the actual advance is
            let scale = 16.0 / units_per_em;
            eprintln!("\nScale factor: {:.10}", scale);
            eprintln!("  615 × scale = {:.10}", 615.0 * scale);
            eprintln!("  616 × scale = {:.10}", 616.0 * scale);
        }
    }
}
