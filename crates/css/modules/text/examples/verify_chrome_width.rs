// This example manually measures "Tab A" with cosmic-text and compares to expected Chrome value
use css_orchestrator::style_model::ComputedStyle;
use css_text::measurement::measure_text;

fn main() {
    let style = ComputedStyle {
        font_size: 16.0,
        font_weight: 400,
        font_family: None, // Will use Liberation Serif
        line_height: None,
        ..Default::default()
    };

    let text = "Tab A";
    let metrics = measure_text(text, &style);

    println!("=== Text Width Measurement ===");
    println!("Text: '{}'", text);
    println!("Font: Liberation Serif 16px weight 400");
    println!();
    println!("Cosmic-text measurement: {:.4}px", metrics.width);
    println!("Chrome expects:          38.4375px");
    println!(
        "Difference:              {:.4}px ({:.2}%)",
        metrics.width - 38.4375,
        ((metrics.width - 38.4375) / 38.4375) * 100.0
    );
    println!();

    // Calculate what the total element width would be
    let padding_border = 22.0; // 10px padding left+right, 1px border left+right
    let cosmic_total = metrics.width + padding_border;
    let chrome_total = 38.4375 + padding_border;

    println!("=== Total Element Width (with padding 10px + border 1px) ===");
    println!("Cosmic-text total: {:.4}px", cosmic_total);
    println!("Chrome total:      60.4375px");
    println!("Difference:        {:.4}px", cosmic_total - chrome_total);

    // Check if cosmic-text might be including something extra
    if (metrics.width - 38.4375).abs() < 1.0 {
        println!("\n✓ Difference is less than 1px - acceptable tolerance");
    } else {
        println!("\n✗ Difference is >= 1px - investigating further...");

        // Try measuring individual characters
        println!("\n=== Character breakdown ===");
        for ch in text.chars() {
            let ch_style = ComputedStyle {
                font_size: 16.0,
                font_weight: 400,
                font_family: None,
                line_height: None,
                ..Default::default()
            };
            let ch_metrics = measure_text(&ch.to_string(), &ch_style);
            println!("'{}': {:.4}px", ch, ch_metrics.width);
        }
    }
}
