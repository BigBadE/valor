#![allow(clippy::unwrap_used)]

use css_box::{compute_box_sides, BoxSides};
use style_engine::{ComputedStyle, Edges};

#[test]
fn compute_box_sides_basic() {
    let mut style = ComputedStyle::default();
    style.margin = Edges {
        top: 10.0,
        right: -5.0,
        bottom: 8.0,
        left: 0.0,
    };
    style.padding = Edges {
        top: 3.5,
        right: 4.0,
        bottom: 2.0,
        left: 1.2,
    };
    style.border_width = Edges {
        top: 2.0,
        right: 2.4,
        bottom: 0.0,
        left: 1.0,
    };

    let sides: BoxSides = compute_box_sides(&style);
    assert_eq!(sides.margin_top, 10);
    assert_eq!(sides.margin_right, -5);
    assert_eq!(sides.margin_bottom, 8);
    assert_eq!(sides.margin_left, 0);

    assert_eq!(sides.padding_top, 3);
    assert_eq!(sides.padding_right, 4);
    assert_eq!(sides.padding_bottom, 2);
    assert_eq!(sides.padding_left, 1);

    assert_eq!(sides.border_top, 2);
    assert_eq!(sides.border_right, 2);
    assert_eq!(sides.border_bottom, 0);
    assert_eq!(sides.border_left, 1);
}
