use renderer::display_list::{DisplayItem, DisplayList};
use wgpu_backend::offscreen::render_display_list_to_rgba;

#[test]
fn test_red_text_pixel_colors() {
    // Create a display list with red text
    let mut items = Vec::new();

    // Pure red text: RGB(255, 0, 0)
    items.push(DisplayItem::Text {
        x: 50.0,
        y: 100.0,
        text: "A".to_string(),
        color: [1.0, 0.0, 0.0], // Pure red
        font_size: 64.0,
        font_weight: 400,
        font_family: Some("Arial".to_string()),
        line_height: 64.0,
        bounds: None,
    });

    let display_list = DisplayList {
        items,
        generation: 0,
    };

    // Render to RGBA buffer
    let width = 200;
    let height = 200;
    let rgba_data = render_display_list_to_rgba(&display_list, width, height)
        .expect("Failed to render display list");

    // The buffer is RGBA format, 4 bytes per pixel
    assert_eq!(rgba_data.len(), (width * height * 4) as usize);

    // Sample pixels from the text area (around x=50-100, y=100)
    // We expect to find pixels with red channel coverage (subpixel antialiasing)
    let mut found_red_coverage = false;
    let mut found_blue_coverage = false;
    let mut found_green_coverage = false;

    // Sample the middle region where text should be
    for y in 80..120 {
        for x in 40..120 {
            let idx = ((y * width + x) * 4) as usize;
            let r = rgba_data[idx];
            let g = rgba_data[idx + 1];
            let b = rgba_data[idx + 2];

            // For red text with subpixel antialiasing, we should see:
            // - High red channel values on left edges
            // - Lower values on other channels
            // If R/B are swapped, we'll see high blue instead

            if r > 100 && g < 100 && b < 50 {
                found_red_coverage = true;
            }
            if b > 100 && g < 100 && r < 50 {
                found_blue_coverage = true;
            }
            if g > 100 {
                found_green_coverage = true;
            }
        }
    }

    eprintln!("Found red coverage: {}", found_red_coverage);
    eprintln!("Found blue coverage: {}", found_blue_coverage);
    eprintln!("Found green coverage: {}", found_green_coverage);

    // Print a few sample pixels for inspection
    eprintln!("\nSample pixels from text area:");
    for y in [90, 95, 100, 105] {
        for x in [50, 60, 70, 80] {
            let idx = ((y * width + x) * 4) as usize;
            let r = rgba_data[idx];
            let g = rgba_data[idx + 1];
            let b = rgba_data[idx + 2];
            let a = rgba_data[idx + 3];
            eprintln!(
                "  Pixel ({:3},{:3}): R={:3} G={:3} B={:3} A={:3}",
                x, y, r, g, b, a
            );
        }
    }

    if found_blue_coverage && !found_red_coverage {
        panic!("ERROR: Found blue coverage instead of red! R and B channels are SWAPPED!");
    }

    if !found_red_coverage && !found_blue_coverage {
        eprintln!(
            "WARNING: No significant red or blue coverage found. Text might not be rendering."
        );
    }
}
