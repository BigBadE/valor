use css_text::measurement::font_system::SystemFontProvider;
use css_text::measurement::metrics::compute_line_height_metrics;

fn main() {
    let font_provider = SystemFontProvider::new();

    // Test case: 14px Liberation Sans (button default font)
    let font_size = 14.0;
    let font_family = "Liberation Sans";

    println!("\n=== Button Height Analysis ===");
    println!("Font: {} at {}px", font_family, font_size);

    // Get line-height metrics for the font
    let metrics =
        compute_line_height_metrics(&font_provider, font_family, font_size, None).unwrap();

    println!("\nLine-height metrics:");
    println!("  Ascent: {:.2}px", metrics.ascent_px);
    println!("  Descent: {:.2}px", metrics.descent_px);
    println!("  Leading: {:.2}px", metrics.leading_px);
    println!(
        "  Normal line-height: {:.2}px",
        metrics.normal_line_height_px
    );

    // Chrome button behavior:
    // - 14px font with 8px padding top/bottom
    // - Total height: 37px
    // - Padding: 16px (8px * 2)
    // - Content height: 37 - 16 = 21px

    let chrome_total_height = 37.0;
    let padding_vertical = 16.0; // 8px top + 8px bottom
    let chrome_content_height = chrome_total_height - padding_vertical;

    println!("\nChrome button (14px font, 8px padding):");
    println!("  Total height: {}px", chrome_total_height);
    println!("  Padding (top+bottom): {}px", padding_vertical);
    println!("  Content height: {}px", chrome_content_height);

    println!("\nValor current behavior:");
    println!(
        "  Content height: {}px (just line-height)",
        metrics.normal_line_height_px
    );
    println!(
        "  Total height: {}px",
        metrics.normal_line_height_px + padding_vertical
    );

    let height_diff = chrome_content_height - metrics.normal_line_height_px;
    println!("\nDifference:");
    println!("  Content height delta: {:.2}px", height_diff);
    println!(
        "  This suggests buttons need {}px intrinsic height BEYOND line-height",
        height_diff
    );

    // Hypothesis: Browsers add extra intrinsic vertical space to buttons
    // Chrome seems to use: line-height + ~5px for 14px Liberation Sans
    // Let's test if this is a constant or ratio
    println!("\nIntrinsic height formula test:");
    println!(
        "  If constant: line-height + 5px = {:.2}px",
        metrics.normal_line_height_px + 5.0
    );
    println!(
        "  If ratio (1.31x): line-height * 1.31 = {:.2}px",
        metrics.normal_line_height_px * 1.31
    );
    println!("  Chrome actual: {}px", chrome_content_height);
}
