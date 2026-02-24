//! Property queries — resolve individual CSS properties to used px values.

use lightningcss::properties::PropertyId;
use lightningcss::properties::display::Display;
use lightningcss::values::length::LengthPercentageOrAuto;
use rewrite_core::{Formula, StylerAccess, Subpixel};

use super::size::content_size_query;

/// Return a formula that resolves a box-model property to its used px value.
pub fn property_query(
    styler: &dyn StylerAccess,
    prop_id: &PropertyId<'static>,
) -> Option<&'static Formula> {
    match prop_id {
        PropertyId::MarginTop => Some(margin_query(styler, MarginSide::Top)),
        PropertyId::MarginRight => Some(margin_query(styler, MarginSide::Right)),
        PropertyId::MarginBottom => Some(margin_query(styler, MarginSide::Bottom)),
        PropertyId::MarginLeft => Some(margin_query(styler, MarginSide::Left)),

        PropertyId::PaddingTop => Some(css_prop!(PaddingTop)),
        PropertyId::PaddingRight => Some(css_prop!(PaddingRight)),
        PropertyId::PaddingBottom => Some(css_prop!(PaddingBottom)),
        PropertyId::PaddingLeft => Some(css_prop!(PaddingLeft)),

        PropertyId::BorderTopWidth => Some(css_prop!(BorderTopWidth)),
        PropertyId::BorderRightWidth => Some(css_prop!(BorderRightWidth)),
        PropertyId::BorderBottomWidth => Some(css_prop!(BorderBottomWidth)),
        PropertyId::BorderLeftWidth => Some(css_prop!(BorderLeftWidth)),

        _ => None,
    }
}

// ============================================================================
// Margin resolution with auto handling
// ============================================================================

#[derive(Clone, Copy)]
enum MarginSide {
    Top,
    Right,
    Bottom,
    Left,
}

impl MarginSide {
    const fn prop_id(self) -> PropertyId<'static> {
        match self {
            Self::Top => PropertyId::MarginTop,
            Self::Right => PropertyId::MarginRight,
            Self::Bottom => PropertyId::MarginBottom,
            Self::Left => PropertyId::MarginLeft,
        }
    }

    const fn is_horizontal(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

fn is_auto(styler: &dyn StylerAccess, prop_id: &PropertyId<'static>) -> bool {
    matches!(
        styler.get_css_property(prop_id),
        Some(lightningcss::properties::Property::MarginLeft(
            LengthPercentageOrAuto::Auto
        )) | Some(lightningcss::properties::Property::MarginRight(
            LengthPercentageOrAuto::Auto
        )) | Some(lightningcss::properties::Property::MarginTop(
            LengthPercentageOrAuto::Auto
        )) | Some(lightningcss::properties::Property::MarginBottom(
            LengthPercentageOrAuto::Auto
        ))
    )
}

fn has_explicit_width(styler: &dyn StylerAccess) -> bool {
    styler.get_css_property(&PropertyId::Width).is_some()
}

fn is_block(styler: &dyn StylerAccess) -> bool {
    matches!(
        styler.get_css_property(&PropertyId::Display),
        Some(lightningcss::properties::Property::Display(
            Display::Pair(pair)
        )) if matches!(
            pair.inside,
            lightningcss::properties::display::DisplayInside::Flow
            | lightningcss::properties::display::DisplayInside::FlowRoot
        ),
    )
}

/// Return a formula for the used margin value on the given side.
fn margin_query(styler: &dyn StylerAccess, side: MarginSide) -> &'static Formula {
    let prop_id = side.prop_id();

    if !is_auto(styler, &prop_id) {
        return match side {
            MarginSide::Top => css_prop!(MarginTop),
            MarginSide::Right => css_prop!(MarginRight),
            MarginSide::Bottom => css_prop!(MarginBottom),
            MarginSide::Left => css_prop!(MarginLeft),
        };
    }

    // Flex item auto margins: delegate to flex module which computes
    // used auto margin values per CSS Flexbox §8.1.
    if let Some(formula) = super::flex::flex_auto_margin_value(styler, &prop_id) {
        return formula;
    }

    // Auto margin — vertical auto margins resolve to 0 per spec (block layout)
    if !side.is_horizontal() {
        return constant!(Subpixel::ZERO);
    }

    // Horizontal auto margin on a block with explicit width
    if is_block(styler) && has_explicit_width(styler) {
        let ml_auto = is_auto(styler, &PropertyId::MarginLeft);
        let mr_auto = is_auto(styler, &PropertyId::MarginRight);

        match (side, ml_auto, mr_auto) {
            // Both auto -> each gets half
            (_, true, true) => {
                return div!(
                    sub!(
                        related!(Parent, content_size_query, rewrite_core::Axis::Horizontal),
                        css_val!(Width),
                    ),
                    constant!(Subpixel::raw(2)),
                );
            }
            // Only left is auto
            (MarginSide::Left, true, false) => {
                return sub!(
                    related!(Parent, content_size_query, rewrite_core::Axis::Horizontal),
                    css_val!(Width),
                    css_prop!(MarginRight),
                );
            }
            // Only right is auto
            (MarginSide::Right, false, true) => {
                return sub!(
                    related!(Parent, content_size_query, rewrite_core::Axis::Horizontal),
                    css_val!(Width),
                    css_prop!(MarginLeft),
                );
            }
            _ => {}
        }
    }

    // Auto margin without explicit width or non-block -> 0
    constant!(Subpixel::ZERO)
}
