use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    println!("=== HOW WE MEASURE TEXT ===\n");

    println!("Code explanation:");
    println!("1. We create a FontSystem (cosmic-text's font manager)");
    println!("2. We create Attrs specifying font family and weight");
    println!("3. We create a Buffer with font-size=16px, line-height=18px");
    println!("4. We call buffer.set_text() which shapes the text");
    println!("5. We call buffer.line_layout() which returns layout info");
    println!("6. The layout_line.w field gives us the width in pixels\n");

    println!("Here's the actual code:\n");
    println!("```rust");
    println!("let mut font_system = FontSystem::new();");
    println!("let attrs = Attrs::new()");
    println!("    .family(Family::Name(\"Liberation Serif\"))");
    println!("    .weight(Weight(400));");
    println!("let buffer_metrics = Metrics::new(16.0, 18.0);");
    println!("let mut buffer = Buffer::new(&mut font_system, buffer_metrics);");
    println!("buffer.set_text(&mut font_system, \"Tab A\", &attrs, Shaping::Advanced, None);");
    println!("buffer.shape_until_scroll(&mut font_system, false);");
    println!("let width = buffer.line_layout(&mut font_system, 0).unwrap()[0].w;");
    println!("```\n");

    println!("=== ACTUAL MEASUREMENTS ===\n");

    let mut font_system = FontSystem::new();

    // Show step-by-step for Liberation Serif
    println!("Measuring 'A' with Liberation Serif:\n");

    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(400));

    println!("Step 1: Create buffer with font-size=16px");
    let buffer_metrics = Metrics::new(16.0, 18.0);
    println!("  buffer_metrics.font_size = {}", buffer_metrics.font_size);

    println!("\nStep 2: Set text and shape");
    let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
    buffer.set_text(&mut font_system, "A", &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(&mut font_system, false);

    println!("  Text shaped successfully");

    println!("\nStep 3: Get layout");
    if let Some(layout) = buffer.line_layout(&mut font_system, 0) {
        let layout_line = &layout[0];
        println!("  layout_line.w = {} px", layout_line.w);
        println!("  This is the glyph advance width returned by cosmic-text");
    }

    println!("\n=== COMPARING DEJAVU FONTS ===\n");

    let fonts = ["DejaVu Sans", "DejaVu Serif", "Liberation Serif"];

    for font in fonts {
        let attrs = Attrs::new().family(Family::Name(font)).weight(Weight(400));
        let buffer_metrics = Metrics::new(16.0, 18.0);

        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, "A", &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        let a_width = buffer.line_layout(&mut font_system, 0).unwrap()[0].w;

        let mut buffer = Buffer::new(&mut font_system, buffer_metrics);
        buffer.set_text(&mut font_system, "B", &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        let b_width = buffer.line_layout(&mut font_system, 0).unwrap()[0].w;

        println!(
            "{:20} A={:.4}px  B={:.4}px  diff={:.4}px",
            font,
            a_width,
            b_width,
            a_width - b_width
        );
    }

    println!("\n=== CLARIFICATION ===\n");
    println!("DejaVu Sans:");
    println!("  A=10.9453px, B=10.9766px");
    println!("  Difference: -0.0312px (B is slightly wider)");
    println!("  Status: ✓ NORMAL - This is how a proper font should look");

    println!("\nDejaVu Serif:");
    println!("  A=11.5547px, B=11.7578px");
    println!("  Difference: -0.2031px (B is slightly wider)");
    println!("  Status: ✓ NORMAL - Letters have varied widths");

    println!("\nLiberation Serif:");
    println!("  A=11.5547px, B=10.6719px");
    println!("  Difference: +0.8828px (A is MUCH wider)");
    println!("  Status: ✗ ABNORMAL - This is the problem!");

    println!("\n=== WHERE THE VALUES COME FROM ===\n");
    println!("cosmic-text (via Rustybuzz) does the following:");
    println!("1. Loads the font file from the system (.ttf or .otf)");
    println!("2. Reads the glyph metrics from the font tables");
    println!("3. Applies text shaping (handles kerning, ligatures, etc.)");
    println!("4. Returns the advance width for each glyph");
    println!();
    println!("The width comes from the 'hmtx' (horizontal metrics) table");
    println!("in the font file, scaled by font-size/units_per_em.");
    println!();
    println!("For Liberation Serif, the glyph 'A' in the font file");
    println!("has an advance width that's ~0.88px wider than 'B'");
    println!("when rendered at 16px font-size.");
}
