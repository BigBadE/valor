use glyphon::{Attrs, Family, FontSystem, fontdb::Weight};

fn main() {
    let mut font_system = FontSystem::new();
    font_system.db_mut().load_system_fonts();

    // Test Liberation Sans regular and bold
    println!("\n=== Liberation Sans ===");
    test_font(&mut font_system, Family::Name("Liberation Sans"), 400);
    test_font(&mut font_system, Family::Name("Liberation Sans"), 600);
    test_font(&mut font_system, Family::Name("Liberation Sans"), 700);

    // Test monospace regular and bold
    println!("\n=== Monospace ===");
    test_font(&mut font_system, Family::Monospace, 400);
    test_font(&mut font_system, Family::Monospace, 600);
    test_font(&mut font_system, Family::Monospace, 700);
}

fn test_font(font_system: &mut FontSystem, family: Family, weight: u16) {
    let attrs = Attrs::new().family(family).weight(glyphon::Weight(weight));
    let matches = font_system.get_font_matches(&attrs);

    if let Some(first) = matches.first() {
        let matched_weight = Weight(first.font_weight);
        if let Some(font) = font_system.get_font(first.id, matched_weight) {
            let metrics = font.metrics();
            let units_per_em = metrics.units_per_em as f32;

            // hhea metrics (always available)
            let hhea_asc = metrics.ascent / units_per_em;
            let hhea_desc = -metrics.descent / units_per_em;
            let hhea_lead = metrics.leading / units_per_em;
            let hhea_total = hhea_asc + hhea_desc;

            println!("\nWeight {} (matched to {}):", weight, first.font_weight);
            println!(
                "  hhea: ascent={:.4}, descent={:.4}, leading={:.4}, total={:.4}",
                hhea_asc, hhea_desc, hhea_lead, hhea_total
            );

            // OS/2 typo metrics
            if let Some((typo_asc, typo_desc, typo_lead, use_typo)) = font.os2_typo_metrics() {
                let typo_total = typo_asc + typo_desc;
                println!(
                    "  OS/2 typo: ascent={:.4}, descent={:.4}, leading={:.4}, total={:.4}, USE_TYPO_METRICS={}",
                    typo_asc, typo_desc, typo_lead, typo_total, use_typo
                );
            } else {
                println!("  OS/2 typo: NOT AVAILABLE");
            }

            // OS/2 win metrics
            if let Some((win_asc, win_desc)) = font.os2_metrics() {
                let win_total = win_asc + win_desc;
                println!(
                    "  OS/2 win: ascent={:.4}, descent={:.4}, total={:.4}",
                    win_asc, win_desc, win_total
                );
            } else {
                println!("  OS/2 win: NOT AVAILABLE");
            }

            // At 13px font size, what would the height be?
            println!("  At 13px font-size:");
            println!("    hhea: {:.2}px", hhea_total * 13.0);
            if let Some((typo_asc, typo_desc, _, use_typo)) = font.os2_typo_metrics() {
                if use_typo {
                    println!(
                        "    OS/2 typo (USE_TYPO_METRICS set): {:.2}px",
                        (typo_asc + typo_desc) * 13.0
                    );
                }
            }
            if let Some((win_asc, win_desc)) = font.os2_metrics() {
                println!("    OS/2 win: {:.2}px", (win_asc + win_desc) * 13.0);
            }
        }
    }
}
