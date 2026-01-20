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

    println!("=== ISOLATING THE EXACT CAUSE ===\n");

    // Individual characters
    println!("Individual characters:");
    let t = measure(&mut font_system, "T");
    let a = measure(&mut font_system, "a");
    let b = measure(&mut font_system, "b");
    let space = measure(&mut font_system, " ");
    let a_upper = measure(&mut font_system, "A");

    println!("  T: {:.4}px", t);
    println!("  a: {:.4}px", a);
    println!("  b: {:.4}px", b);
    println!("  space: {:.4}px", space);
    println!("  A: {:.4}px", a_upper);
    println!("  Sum (T+a+b+space): {:.4}px\n", t + a + b + space);

    // Combinations without space
    println!("Combinations WITHOUT space:");
    let tab = measure(&mut font_system, "Tab");
    println!("  'Tab': {:.4}px", tab);
    println!("  Expected (T+a+b): {:.4}px", t + a + b);
    println!("  Kerning: {:.4}px\n", tab - (t + a + b));

    // Combinations WITH space at the end
    println!("Combinations WITH space at end:");
    let tab_space = measure(&mut font_system, "Tab ");
    println!("  'Tab ': {:.4}px", tab_space);
    println!("  Expected (T+a+b+space): {:.4}px", t + a + b + space);
    println!("  Kerning: {:.4}px", tab_space - (t + a + b + space));
    println!("  Difference from 'Tab': {:.4}px\n", tab_space - tab);

    // Just space in different contexts
    println!("Space in different contexts:");
    let a_space = measure(&mut font_system, "a ");
    let b_space = measure(&mut font_system, "b ");
    let t_space = measure(&mut font_system, "T ");

    println!(
        "  'a ': {:.4}px (expected a+space: {:.4}px, diff: {:.4}px)",
        a_space,
        a + space,
        a_space - (a + space)
    );
    println!(
        "  'b ': {:.4}px (expected b+space: {:.4}px, diff: {:.4}px)",
        b_space,
        b + space,
        b_space - (b + space)
    );
    println!(
        "  'T ': {:.4}px (expected T+space: {:.4}px, diff: {:.4}px)\n",
        t_space,
        t + space,
        t_space - (t + space)
    );

    // Two-character combinations
    println!("Two-character combinations:");
    let ta = measure(&mut font_system, "Ta");
    let ab = measure(&mut font_system, "ab");

    println!(
        "  'Ta': {:.4}px (expected T+a: {:.4}px, kerning: {:.4}px)",
        ta,
        t + a,
        ta - (t + a)
    );
    println!(
        "  'ab': {:.4}px (expected a+b: {:.4}px, kerning: {:.4}px)\n",
        ab,
        a + b,
        ab - (a + b)
    );

    // Chrome comparison
    println!("=== CHROME COMPARISON ===\n");

    // We know:
    // Chrome's "Tab A" = 38.4375px
    // Our "A" = 11.5547px (matches Chrome exactly)
    // Therefore Chrome's "Tab " = 38.4375 - 11.5547 = 26.8828px

    let chrome_tab_a = 38.4375;
    let chrome_tab_space = 26.8828;

    println!("Chrome's 'Tab ': {:.4}px", chrome_tab_space);
    println!("Our 'Tab ': {:.4}px", tab_space);
    println!("Difference: {:.4}px\n", tab_space - chrome_tab_space);

    // Now test if "Tab" (without space) also has the issue
    println!("Does 'Tab' (no space) have the same ratio?");
    let chrome_ratio = chrome_tab_space / tab_space;
    let implied_chrome_tab = tab * chrome_ratio;
    println!("  Our 'Tab': {:.4}px", tab);
    println!("  If Chrome has same ratio: {:.4}px", implied_chrome_tab);
    println!("  Ratio: {:.6}\n", chrome_ratio);

    // Test each character scaled by the ratio
    println!(
        "If we scale each character by the ratio ({:.6}):",
        chrome_ratio
    );
    println!(
        "  T would be: {:.4}px (currently {:.4}px)",
        t * chrome_ratio,
        t
    );
    println!(
        "  a would be: {:.4}px (currently {:.4}px)",
        a * chrome_ratio,
        a
    );
    println!(
        "  b would be: {:.4}px (currently {:.4}px)",
        b * chrome_ratio,
        b
    );
    println!(
        "  space would be: {:.4}px (currently {:.4}px)",
        space * chrome_ratio,
        space
    );
    println!("  Sum: {:.4}px\n", (t + a + b + space) * chrome_ratio);

    // Test if the error is proportional or absolute
    println!("=== ERROR DISTRIBUTION ===\n");
    let error_tab_space = tab_space - chrome_tab_space;
    println!("Total error in 'Tab ': {:.4}px", error_tab_space);
    println!(
        "Error per character (if distributed evenly): {:.4}px",
        error_tab_space / 4.0
    );

    // Check if error is in specific character or kerning
    println!("\nHypothesis testing:");
    println!(
        "If error is in 'T': T would need to be {:.4}px",
        t - error_tab_space
    );
    println!(
        "If error is in 'a': a would need to be {:.4}px",
        a - error_tab_space
    );
    println!(
        "If error is in 'b': b would need to be {:.4}px",
        b - error_tab_space
    );
    println!(
        "If error is in space: space would need to be {:.4}px",
        space - error_tab_space
    );
    println!(
        "If error is in kerning: kerning would need to be {:.4}px",
        (tab - (t + a + b)) - error_tab_space
    );
}
