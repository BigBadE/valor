use glyphon::cosmic_text::{Attrs, Family, FontSystem, Weight};

fn main() {
    let mut font_system = FontSystem::new();

    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(400));

    // Get the font matches
    let font_matches = font_system.get_font_matches(&attrs);

    println!("=== Font matching for Liberation Serif ===");
    for (i, font_match) in font_matches.iter().enumerate() {
        println!("\nMatch {}:", i);
        println!("  Font ID: {:?}", font_match.id);

        // Try to get font info
        if let Some(font) = font_system.get_font(font_match.id) {
            let font = font.as_swash();
            let metrics = font.metrics(&[]);

            println!("  Units per em: {}", metrics.units_per_em);
            println!("  Ascent: {}", metrics.ascent);
            println!("  Descent: {}", metrics.descent);
            println!("  Leading: {}", metrics.leading);

            // In a font, the em-size is the units_per_em
            // When we specify font-size 16px, we're saying:
            // "scale the font so that 1em = 16px"
            // So if units_per_em = 1000, then at 16px:
            // - 1 font unit = 16/1000 = 0.016px

            let font_size_px = 16.0;
            let scale = font_size_px / metrics.units_per_em as f32;

            println!("\n  At font-size 16px:");
            println!("    Scale factor: {:.6}", scale);
            println!("    Ascent in px: {:.4}", metrics.ascent as f32 * scale);
            println!("    Descent in px: {:.4}", metrics.descent as f32 * scale);
            println!("    Leading in px: {:.4}", metrics.leading as f32 * scale);

            // Now let's check glyph advance for 'T'
            let glyph_id = font.charmap().map('T');
            if let Some(glyph_id) = glyph_id {
                println!("\n  Glyph 'T' (id={}):", glyph_id);
                if let Some(advance) = font.glyph_metrics(&[]).advance_width(glyph_id) {
                    println!("    Advance width (font units): {}", advance);
                    println!("    Advance width at 16px: {:.4}px", advance as f32 * scale);
                }
            }

            // Check other characters
            for ch in ['a', 'b', ' '] {
                if let Some(glyph_id) = font.charmap().map(ch) {
                    if let Some(advance) = font.glyph_metrics(&[]).advance_width(glyph_id) {
                        println!(
                            "  Glyph '{}' advance at 16px: {:.4}px",
                            ch,
                            advance as f32 * scale
                        );
                    }
                }
            }
        }
    }

    // Check if Chrome might be using a different units_per_em interpretation
    println!("\n=== Hypothesis: Chrome uses different scaling ===");
    // If "Tab " should be 26.8828px but we get 27.7578px at font-size 16px,
    // the ratio is 26.8828 / 27.7578 = 0.968477
    // This means Chrome might be scaling by 16 * 0.968477 = 15.4956px
    // OR Chrome's font has different metrics

    let ratio = 26.8828 / 27.7578;
    println!("Width ratio (Chrome/Ours): {:.6}", ratio);
    println!("If font units_per_em were scaled by this ratio:");
    println!(
        "  Effective units_per_em would be: 2048 / {:.6} = {:.2}",
        ratio,
        2048.0 / ratio
    );
    println!(
        "  Or equivalently, effective font-size: 16 * {:.6} = {:.4}px",
        ratio,
        16.0 * ratio
    );
}
