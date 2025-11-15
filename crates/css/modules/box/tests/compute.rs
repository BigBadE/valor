use css_box::compute_box_sides;
use css_orchestrator::style_model::{BorderWidths, ComputedStyle, Edges};

#[cfg(test)]
mod tests {
    use super::*;

    /// Test basic box sides computation from `ComputedStyle`.
    ///
    /// # Panics
    /// Panics if computed box sides do not match expected values.
    #[test]
    fn compute_box_sides_basic() {
        let style = ComputedStyle {
            margin: Edges {
                top: 10.0,
                right: -5.0,
                bottom: 8.0,
                left: 0.0,
            },
            padding: Edges {
                top: 3.5,
                right: 4.0,
                bottom: 2.0,
                left: 1.2,
            },
            border_width: BorderWidths {
                top: 2.0,
                right: 2.4,
                bottom: 0.0,
                left: 1.0,
            },
            ..Default::default()
        };

        let sides = compute_box_sides(&style);
        assert_eq!(sides.margin_top, 10i32);
        assert_eq!(sides.margin_right, -5i32);
        assert_eq!(sides.margin_bottom, 8i32);
        assert_eq!(sides.margin_left, 0i32);

        assert_eq!(sides.padding_top, 3i32);
        assert_eq!(sides.padding_right, 4i32);
        assert_eq!(sides.padding_bottom, 2i32);
        assert_eq!(sides.padding_left, 1i32);

        assert_eq!(sides.border_top, 2i32);
        assert_eq!(sides.border_right, 2i32);
        assert_eq!(sides.border_bottom, 0i32);
        assert_eq!(sides.border_left, 1i32);
    }
}
