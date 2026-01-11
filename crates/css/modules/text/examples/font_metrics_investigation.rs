use glyphon::{Attrs, Family, FontSystem, Weight};

fn main() {
    let mut font_system = FontSystem::new();
    font_system.db_mut().load_system_fonts();

    // Test with Liberation Serif (Times New Roman equivalent on Linux)
    // This is what the basic_text.html fixture uses (default serif font)
    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(400));

    let font_matches = font_system.get_font_matches(&attrs);
    let first_match = font_matches.first().unwrap();
    let font = font_system
        .get_font(
            first_match.id,
            glyphon::fontdb::Weight(first_match.font_weight),
        )
        .unwrap();

    let metrics = font.metrics();
    let units_per_em = f32::from(metrics.units_per_em);

    println!("=== Liberation Serif Font Metrics (Times New Roman equivalent, weight 400) ===\n");
    println!("units_per_em: {}\n", units_per_em);

    // hhea table metrics
    let hhea_ascent = metrics.ascent / units_per_em;
    let hhea_descent = -metrics.descent / units_per_em;
    let hhea_leading = metrics.leading / units_per_em;

    println!("hhea table:");
    println!("  ascent:  {} ({:.6})", metrics.ascent, hhea_ascent);
    println!("  descent: {} ({:.6})", metrics.descent, hhea_descent);
    println!("  leading: {} ({:.6})", metrics.leading, hhea_leading);
    println!(
        "  total:   {:.6}",
        hhea_ascent + hhea_descent + hhea_leading
    );

    // OS/2 typo metrics
    if let Some((typo_ascent, typo_descent, typo_leading, use_typo)) = font.os2_typo_metrics() {
        println!("\nOS/2 typo table:");
        println!("  ascent:  {:.6}", typo_ascent);
        println!("  descent: {:.6}", typo_descent);
        println!("  leading: {:.6}", typo_leading);
        println!(
            "  total:   {:.6}",
            typo_ascent + typo_descent + typo_leading
        );
        println!("  USE_TYPO_METRICS flag: {}", use_typo);
    }

    // OS/2 win metrics
    if let Some((win_ascent, win_descent)) = font.os2_metrics() {
        println!("\nOS/2 win table:");
        println!("  ascent:  {:.6}", win_ascent);
        println!("  descent: {:.6}", win_descent);
        println!("  total:   {:.6} (no line gap)", win_ascent + win_descent);
    }

    println!("\n=== Line-height calculations for common font sizes ===\n");

    for font_size in [12.0, 14.0, 16.0, 18.0, 20.0, 24.0] {
        println!("Font size: {}px", font_size);

        // hhea calculation
        let hhea_lh = (hhea_ascent + hhea_descent + hhea_leading) * font_size;
        println!("  hhea: {:.2} (rounded: {})", hhea_lh, hhea_lh.round());

        // OS/2 typo calculation
        if let Some((typo_ascent, typo_descent, typo_leading, _)) = font.os2_typo_metrics() {
            let typo_lh = (typo_ascent + typo_descent + typo_leading) * font_size;
            println!("  typo: {:.2} (rounded: {})", typo_lh, typo_lh.round());
        }

        // OS/2 win calculation
        if let Some((win_ascent, win_descent)) = font.os2_metrics() {
            let win_lh = (win_ascent + win_descent) * font_size;
            println!("  win:  {:.2} (rounded: {})", win_lh, win_lh.round());
        }

        // Chrome observed values from basic_text.html fixture
        let chrome_expected = match font_size as i32 {
            12 => 17,
            14 => 21,
            16 => 24,
            18 => 27,
            20 => 30,
            24 => 27, // From the failing test - container height for 24px font
            _ => 0,
        };

        println!("  Chrome observed: {}", chrome_expected);

        // Now let's try to reverse engineer what Chrome might be doing
        // Calculate what multiplier Chrome is using
        let chrome_multiplier = chrome_expected as f32 / font_size;
        println!("  Chrome multiplier: {:.6}x", chrome_multiplier);

        // What if Chrome is using ceil() instead of round()?
        println!("  hhea ceil: {}", hhea_lh.ceil());
        if let Some((typo_ascent, typo_descent, typo_leading, _)) = font.os2_typo_metrics() {
            let typo_lh = (typo_ascent + typo_descent + typo_leading) * font_size;
            println!("  typo ceil: {}", typo_lh.ceil());
        }

        // What if Chrome multiplies the metrics by something?
        println!("  hhea * 1.2: {:.2}", hhea_lh * 1.2);
        println!("  hhea * 1.3: {:.2}", hhea_lh * 1.3);

        println!();
    }
}
