// This is a simple test to check what font_size value is actually used
// We'll create a ComputedStyle and check its font_size field

use css_orchestrator::style_model::ComputedStyle;

fn main() {
    let style = ComputedStyle::default();

    println!("Default ComputedStyle font_size: {}", style.font_size);
    println!("As f32 bits: {:032b}", style.font_size.to_bits());

    // Check if 16.0 is exactly representable
    let sixteen = 16.0f32;
    println!("\n16.0f32 bits: {:032b}", sixteen.to_bits());
    println!("Are they equal? {}", style.font_size == sixteen);

    // Check what 15.5 would be
    let fifteen_point_five = 15.5f32;
    println!("\n15.5f32: {}", fifteen_point_five);
    println!("15.5f32 bits: {:032b}", fifteen_point_five.to_bits());
}
