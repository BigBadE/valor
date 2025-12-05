use css_box::{LayoutUnit, compute_box_sides};
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
        // Values are now in LayoutUnit (1/64px units)
        assert_eq!(sides.margin_top, LayoutUnit::from_px(10.0));
        assert_eq!(sides.margin_right, LayoutUnit::from_px(-5.0));
        assert_eq!(sides.margin_bottom, LayoutUnit::from_px(8.0));
        assert_eq!(sides.margin_left, LayoutUnit::from_px(0.0));

        assert_eq!(sides.padding_top, LayoutUnit::from_px(3.5));
        assert_eq!(sides.padding_right, LayoutUnit::from_px(4.0));
        assert_eq!(sides.padding_bottom, LayoutUnit::from_px(2.0));
        assert!((sides.padding_left.to_px() - 1.2).abs() < 0.01); // Approximate due to rounding

        assert_eq!(sides.border_top, LayoutUnit::from_px(2.0));
        assert!((sides.border_right.to_px() - 2.4).abs() < 0.01); // Approximate due to rounding
        assert_eq!(sides.border_bottom, LayoutUnit::from_px(0.0));
        assert_eq!(sides.border_left, LayoutUnit::from_px(1.0));
    }
}
