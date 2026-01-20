use glyphon::cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};

fn main() {
    let font_sys = &mut FontSystem::new();
    let font_size = 16.0;
    let line_height = 18.0;

    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(400));

    let buffer_metrics = Metrics::new(font_size, line_height);

    // Chrome expects "Tab " = 26.8828px
    // We measure "Tab " = 27.7578px
    // Difference: 0.875px

    println!("=== Testing each character individually ===");
    for ch in ['T', 'a', 'b', ' '] {
        let text = ch.to_string();
        let mut buffer = Buffer::new(font_sys, buffer_metrics);
        buffer.set_text(font_sys, &text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(font_sys, false);

        let width = buffer
            .layout_runs()
            .flat_map(|run| run.glyphs.iter())
            .map(|g| g.w)
            .sum::<f32>();

        println!("{:?}: {:.4}px", ch, width);
    }

    println!("\n=== Testing pairs ===");
    for pair in ["Ta", "ab", "b "] {
        let mut buffer = Buffer::new(font_sys, buffer_metrics);
        buffer.set_text(font_sys, pair, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(font_sys, false);

        let width = buffer
            .layout_runs()
            .flat_map(|run| run.glyphs.iter())
            .map(|g| g.w)
            .sum::<f32>();

        println!("{:?}: {:.4}px", pair, width);
    }

    println!("\n=== Testing cumulative ===");
    for text in ["T", "Ta", "Tab", "Tab "] {
        let mut buffer = Buffer::new(font_sys, buffer_metrics);
        buffer.set_text(font_sys, text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(font_sys, false);

        let width = buffer
            .layout_runs()
            .flat_map(|run| run.glyphs.iter())
            .map(|g| g.w)
            .sum::<f32>();

        println!("{:?}: {:.4}px", text, width);
    }

    println!("\n=== Testing if difference is distributed ===");
    // If 0.875px is distributed across the 4 characters, each would be ~0.21875px narrower
    // T: 9.7734 - 0.21875 = 9.5547
    // a: 7.1016 - 0.21875 = 6.8828
    // b: 8.0000 - 0.21875 = 7.7812
    // space: 4.0000 - 0.21875 = 3.7812
    println!("If distributed evenly:");
    println!("  T would be: 9.5547px");
    println!("  a would be: 6.8828px");
    println!("  b would be: 7.7812px");
    println!("  space would be: 3.7812px");

    println!("\n=== Testing if difference is in space ===");
    // If all 0.875px is in the space character:
    println!("If all in space: space would be: 3.1250px (currently 4.0000px)");

    println!("\n=== Testing space character in different contexts ===");
    for text in [" ", "a ", "b ", "ab ", "Tab "] {
        let mut buffer = Buffer::new(font_sys, buffer_metrics);
        buffer.set_text(font_sys, text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(font_sys, false);

        let width = buffer
            .layout_runs()
            .flat_map(|run| run.glyphs.iter())
            .map(|g| g.w)
            .sum::<f32>();

        println!("{:?}: {:.4}px", text, width);
    }
}
