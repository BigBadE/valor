//! Individual CSS property query types.
//!
//! These provide convenient type-safe access to CSS properties through the query system.
//! Each query type internally queries the InheritedCssPropertyQuery with the correct property name.

use crate::CssValue;
use crate::storage::InheritedCssPropertyQuery;
use rewrite_core::{Database, DependencyContext, NodeId, Query};

// Macro to generate property query types
macro_rules! property_query {
    ($name:ident, $property:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name;

        impl Query for $name {
            type Key = NodeId;
            type Value = CssValue;

            fn execute(db: &Database, node: Self::Key, ctx: &mut DependencyContext) -> Self::Value {
                db.query::<InheritedCssPropertyQuery>((node, $property.to_string()), ctx)
            }
        }
    };
}

// Display & Visibility
property_query!(DisplayQuery, "display");
property_query!(VisibilityQuery, "visibility");
property_query!(OpacityQuery, "opacity");

// Positioning
property_query!(PositionQuery, "position");
property_query!(ZIndexQuery, "z-index");

// Float & Clear
property_query!(FloatQuery, "float");
property_query!(ClearQuery, "clear");

// Overflow
property_query!(OverflowQuery, "overflow");
property_query!(OverflowXQuery, "overflow-x");
property_query!(OverflowYQuery, "overflow-y");

// Flexbox Container
property_query!(FlexDirectionQuery, "flex-direction");
property_query!(FlexWrapQuery, "flex-wrap");
property_query!(JustifyContentQuery, "justify-content");
property_query!(AlignItemsQuery, "align-items");
property_query!(AlignContentQuery, "align-content");

// Flexbox Item
property_query!(FlexBasisQuery, "flex-basis");
// Note: FlexGrow and FlexShrink are in dimensional.rs as they return Subpixels
property_query!(AlignSelfQuery, "align-self");
property_query!(OrderQuery, "order");

// Grid Container
property_query!(GridTemplateColumnsQuery, "grid-template-columns");
property_query!(GridTemplateRowsQuery, "grid-template-rows");
property_query!(GridTemplateAreasQuery, "grid-template-areas");
property_query!(GridAutoColumnsQuery, "grid-auto-columns");
property_query!(GridAutoRowsQuery, "grid-auto-rows");
property_query!(GridAutoFlowQuery, "grid-auto-flow");
property_query!(JustifyItemsQuery, "justify-items");

// Grid Item
property_query!(GridColumnStartQuery, "grid-column-start");
property_query!(GridColumnEndQuery, "grid-column-end");
property_query!(GridRowStartQuery, "grid-row-start");
property_query!(GridRowEndQuery, "grid-row-end");
property_query!(GridColumnQuery, "grid-column");
property_query!(GridRowQuery, "grid-row");
property_query!(GridAreaQuery, "grid-area");
property_query!(JustifySelfQuery, "justify-self");

// Typography - Font
property_query!(FontFamilyQuery, "font-family");
property_query!(FontSizeQuery, "font-size");
property_query!(FontWeightQuery, "font-weight");
property_query!(FontStyleQuery, "font-style");
property_query!(FontVariantQuery, "font-variant");
property_query!(FontStretchQuery, "font-stretch");
property_query!(LineHeightQuery, "line-height");

// Typography - Text
property_query!(ColorQuery, "color");
property_query!(TextAlignQuery, "text-align");
property_query!(TextDecorationQuery, "text-decoration");
property_query!(TextTransformQuery, "text-transform");
property_query!(TextIndentQuery, "text-indent");
property_query!(TextOverflowQuery, "text-overflow");

// Typography - White Space
property_query!(WhiteSpaceQuery, "white-space");
property_query!(WordBreakQuery, "word-break");
property_query!(WordWrapQuery, "word-wrap");
property_query!(LetterSpacingQuery, "letter-spacing");

// Background
property_query!(BackgroundColorQuery, "background-color");
property_query!(BackgroundImageQuery, "background-image");

// Table
property_query!(TableLayoutQuery, "table-layout");
property_query!(BorderCollapseQuery, "border-collapse");
property_query!(BorderSpacingQuery, "border-spacing");

// Note: RowGap, ColumnGap, Gap are in dimensional.rs as they return Subpixels

// Writing Modes
property_query!(WritingModeQuery, "writing-mode");
property_query!(DirectionQuery, "direction");

// Other
property_query!(VerticalAlignQuery, "vertical-align");
property_query!(BoxSizingQuery, "box-sizing");
