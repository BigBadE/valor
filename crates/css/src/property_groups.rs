//! Property group classification for layout invalidation.

use lightningcss::properties::{Property, PropertyId};

/// Check if a property affects layout position/offset.
pub fn affects_position(property: &Property<'static>) -> bool {
    matches!(
        property.property_id(),
        PropertyId::Display
            | PropertyId::Position
            | PropertyId::Top
            | PropertyId::Right
            | PropertyId::Bottom
            | PropertyId::Left
            | PropertyId::FlexDirection(_)
            | PropertyId::FlexWrap(_)
            | PropertyId::JustifyContent(_)
            | PropertyId::AlignItems(_)
            | PropertyId::AlignContent(_)
            | PropertyId::Order(_)
            | PropertyId::GridTemplateColumns
            | PropertyId::GridTemplateRows
            | PropertyId::GridColumn
            | PropertyId::GridRow
            | PropertyId::GridAutoFlow
    )
}

/// Check if a property affects layout size.
pub fn affects_size(property: &Property<'static>) -> bool {
    matches!(
        property.property_id(),
        PropertyId::Display
            | PropertyId::Width
            | PropertyId::Height
            | PropertyId::MinWidth
            | PropertyId::MinHeight
            | PropertyId::MaxWidth
            | PropertyId::MaxHeight
            | PropertyId::Padding
            | PropertyId::PaddingTop
            | PropertyId::PaddingRight
            | PropertyId::PaddingBottom
            | PropertyId::PaddingLeft
            | PropertyId::Margin
            | PropertyId::MarginTop
            | PropertyId::MarginRight
            | PropertyId::MarginBottom
            | PropertyId::MarginLeft
            | PropertyId::BorderWidth
            | PropertyId::BorderTopWidth
            | PropertyId::BorderRightWidth
            | PropertyId::BorderBottomWidth
            | PropertyId::BorderLeftWidth
            | PropertyId::BoxSizing(_)
            | PropertyId::FlexGrow(_)
            | PropertyId::FlexShrink(_)
            | PropertyId::FlexBasis(_)
            | PropertyId::GridTemplateColumns
            | PropertyId::GridTemplateRows
    )
}
