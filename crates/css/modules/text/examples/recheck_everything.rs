use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn measure(font_system: &mut FontSystem, text: &str) -> f32 {
    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(400));
    let buffer_metrics = Metrics::new(16.0, 18.0);
    let mut buffer = Buffer::new(font_system, buffer_metrics);
    buffer.set_text(font_system, text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    buffer.line_layout(font_system, 0).unwrap()[0].w
}

fn main() {
    let mut font_system = FontSystem::new();

    println!("=== RECHECKING EVERYTHING CAREFULLY ===\n");

    // Our measurements
    let our_tab_a = measure(&mut font_system, "Tab A");
    let our_tab_b = measure(&mut font_system, "Tab B");
    let our_tab_d = measure(&mut font_system, "Tab D");
    let our_a = measure(&mut font_system, "A");
    let our_b = measure(&mut font_system, "B");
    let our_d = measure(&mut font_system, "D");
    let our_tab_space = measure(&mut font_system, "Tab ");

    // Chrome's measurements from layout JSON
    let chrome_tab_a = 38.4375;
    let chrome_tab_b = 38.4375;
    let chrome_tab_d = 39.3125;

    println!("CHROME'S MEASUREMENTS:");
    println!("  Tab A: {:.4}px", chrome_tab_a);
    println!("  Tab B: {:.4}px", chrome_tab_b);
    println!("  Tab D: {:.4}px", chrome_tab_d);

    println!("\nOUR MEASUREMENTS:");
    println!("  Tab A: {:.4}px", our_tab_a);
    println!("  Tab B: {:.4}px", our_tab_b);
    println!("  Tab D: {:.4}px", our_tab_d);

    println!("\nDIFFERENCES:");
    println!("  Tab A error: {:.4}px", our_tab_a - chrome_tab_a);
    println!("  Tab B error: {:.4}px", our_tab_b - chrome_tab_b);
    println!("  Tab D error: {:.4}px", our_tab_d - chrome_tab_d);

    println!("\n=== BREAKING DOWN Tab A ===");
    println!("Our:");
    println!("  'Tab ': {:.4}px", our_tab_space);
    println!("  'A': {:.4}px", our_a);
    println!("  'Tab ' + 'A': {:.4}px", our_tab_space + our_a);
    println!("  Actual 'Tab A': {:.4}px", our_tab_a);
    println!(
        "  Match? {}",
        if (our_tab_a - (our_tab_space + our_a)).abs() < 0.001 {
            "YES"
        } else {
            "NO"
        }
    );

    println!("\nChrome:");
    println!("  'Tab A': {:.4}px", chrome_tab_a);
    println!("  If we assume 'A' = {:.4}px (same as ours)", our_a);
    println!(
        "  Then Chrome's 'Tab ' would be: {:.4}px",
        chrome_tab_a - our_a
    );
    println!("  Our 'Tab ': {:.4}px", our_tab_space);
    println!(
        "  Difference in 'Tab ': {:.4}px",
        our_tab_space - (chrome_tab_a - our_a)
    );

    println!("\n=== BREAKING DOWN Tab B ===");
    println!("Our:");
    println!("  'Tab ': {:.4}px", our_tab_space);
    println!("  'B': {:.4}px", our_b);
    println!("  'Tab ' + 'B': {:.4}px", our_tab_space + our_b);
    println!("  Actual 'Tab B': {:.4}px", our_tab_b);
    println!(
        "  Match? {}",
        if (our_tab_b - (our_tab_space + our_b)).abs() < 0.001 {
            "YES"
        } else {
            "NO"
        }
    );

    println!("\nChrome:");
    println!("  'Tab B': {:.4}px", chrome_tab_b);
    println!("  If we assume 'B' = {:.4}px (same as ours)", our_b);
    println!(
        "  Then Chrome's 'Tab ' would be: {:.4}px",
        chrome_tab_b - our_b
    );
    println!("  Our 'Tab ': {:.4}px", our_tab_space);
    println!(
        "  Difference in 'Tab ': {:.4}px",
        our_tab_space - (chrome_tab_b - our_b)
    );

    println!("\n=== BREAKING DOWN Tab D ===");
    println!("Our:");
    println!("  'Tab ': {:.4}px", our_tab_space);
    println!("  'D': {:.4}px", our_d);
    println!("  'Tab ' + 'D': {:.4}px", our_tab_space + our_d);
    println!("  Actual 'Tab D': {:.4}px", our_tab_d);
    println!(
        "  Match? {}",
        if (our_tab_d - (our_tab_space + our_d)).abs() < 0.001 {
            "YES"
        } else {
            "NO"
        }
    );

    println!("\nChrome:");
    println!("  'Tab D': {:.4}px", chrome_tab_d);
    println!("  If we assume 'D' = {:.4}px (same as ours)", our_d);
    println!(
        "  Then Chrome's 'Tab ' would be: {:.4}px",
        chrome_tab_d - our_d
    );
    println!("  Our 'Tab ': {:.4}px", our_tab_space);
    println!(
        "  Difference in 'Tab ': {:.4}px",
        our_tab_space - (chrome_tab_d - our_d)
    );

    println!("\n=== CRITICAL OBSERVATION ===");
    println!("If 'A', 'B', 'D' all have the same widths in our font and Chrome's,");
    println!("then the implied Chrome 'Tab ' would be:");
    println!("  From Tab A: {:.4}px", chrome_tab_a - our_a);
    println!("  From Tab B: {:.4}px", chrome_tab_b - our_b);
    println!("  From Tab D: {:.4}px", chrome_tab_d - our_d);
    println!("\nThese should all be the same IF the letters have the same widths.");
    println!("But they're NOT the same! This means the LETTERS have different widths.");
}
