use css_text::measurement::font_system::{get_font_metrics, get_font_system};
use glyphon::{Attrs, Family, Weight};

fn main() {
    let font_system_arc = get_font_system();
    let mut font_system = font_system_arc.lock().unwrap();

    let attrs = Attrs::new()
        .family(Family::Name("Liberation Serif"))
        .weight(Weight(700));

    if let Some(metrics) = get_font_metrics(&mut font_system, &attrs) {
        let font_size = 18.72;
        let ascent_px = metrics.ascent * font_size;
        let descent_px = metrics.descent * font_size;
        let leading_px = metrics.leading * font_size;
        let total = ascent_px + descent_px + leading_px;

        println!("Liberation Serif Bold metrics:");
        println!("  ascent: {:.6}", metrics.ascent);
        println!("  descent: {:.6}", metrics.descent);
        println!("  leading: {:.6}", metrics.leading);
        println!();
        println!("At 18.72px:");
        println!("  ascent: {:.4}px", ascent_px);
        println!("  descent: {:.4}px", descent_px);
        println!("  leading: {:.4}px", leading_px);
        println!("  total: {:.4}px", total);
        println!("  fractional part: {:.4}", total.fract());
        println!();

        let line_height = if total.fract() < 0.65 {
            println!(
                "  Threshold rounding: {:.4} < 0.65 → floor()",
                total.fract()
            );
            total.floor()
        } else {
            println!(
                "  Threshold rounding: {:.4} >= 0.65 → ceil()",
                total.fract()
            );
            total.ceil()
        };

        println!("  line-height: {:.0}px", line_height);
        println!();
        println!("Chrome expects: 21px text, 22px element");
        println!("Difference: {} - 22 = {}", line_height, line_height - 22.0);
    }
}
