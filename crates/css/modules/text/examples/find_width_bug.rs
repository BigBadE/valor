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

    println!("=== Testing Individual Characters ===");
    for ch in &["T", "a", "b", " ", "A"] {
        let w = measure(&mut font_system, ch);
        println!("{:5}: {:.4}px", format!("'{}'", ch), w);
    }

    println!("\n=== Testing Character Pairs ===");
    let pairs = vec!["Ta", "ab", "b ", " A"];
    for pair in &pairs {
        let w = measure(&mut font_system, pair);
        println!("{:5}: {:.4}px", format!("'{}'", pair), w);
    }

    println!("\n=== Testing Progressively Building 'Tab A' ===");
    let builds = vec!["T", "Ta", "Tab", "Tab ", "Tab A"];
    for text in &builds {
        let w = measure(&mut font_system, text);
        println!("{:10}: {:.4}px", format!("'{}'", text), w);
    }

    println!("\n=== Comparing Tab A vs Tab D ===");
    let tab_a = measure(&mut font_system, "Tab A");
    let tab_d = measure(&mut font_system, "Tab D");
    println!("Tab A: {:.4}px", tab_a);
    println!("Tab D: {:.4}px", tab_d);
    println!("Diff:  {:.4}px", tab_a - tab_d);

    println!("\n=== Expected from Chrome ===");
    println!("Tab A: 38.4375px");
    println!("Tab D: 39.3125px");
    println!("\n=== Our measurements ===");
    println!(
        "Tab A: {:.4}px (expected 38.4375, diff {:.4}px)",
        tab_a,
        tab_a - 38.4375
    );
    println!(
        "Tab D: {:.4}px (expected 39.3125, diff {:.4}px)",
        tab_d,
        tab_d - 39.3125
    );
}
