//! Property group classification.
//!
//! Maps CSS `PropertyId` variants to one of five property groups,
//! each backed by its own sparse tree. Properties not in any group
//! fall through to the legacy per-node store.

use lightningcss::properties::PropertyId;

/// Which sparse tree a CSS property belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyGroup {
    /// Font, color, text-align, line-height, etc.
    /// Inherited: yes — query walks up the sparse tree on miss.
    Text,
    /// Background-color, border-color, border-style, border-radius,
    /// box-shadow, opacity, outline.
    /// Inherited: no.
    Background,
    /// Width, height, min/max sizing, margin, padding, border-width, box-sizing.
    /// Inherited: no.
    BoxModel,
    /// Display, flex-*, grid-*, gap, order, align-*, justify-*.
    /// Inherited: no.
    Layout,
    /// Position, top/right/bottom/left, z-index.
    /// Inherited: no.
    Position,
}

impl PropertyGroup {
    /// Whether this group's properties are inherited per the CSS spec.
    /// When `true`, a query miss on a node walks to its sparse-tree parent.
    pub const fn is_inherited(self) -> bool {
        matches!(self, Self::Text)
    }
}

/// Classify a `PropertyId` into its property group.
///
/// Returns `None` for properties we don't track in any sparse tree
/// (they stay in the legacy fallback store).
#[allow(
    clippy::too_many_lines,
    reason = "single match on all PropertyId variants"
)]
pub fn classify(prop_id: &PropertyId<'static>) -> Option<PropertyGroup> {
    match prop_id {
        // ── Text (inherited) ──────────────────────────────────────
        PropertyId::FontFamily
        | PropertyId::FontSize
        | PropertyId::FontWeight
        | PropertyId::FontStyle
        | PropertyId::FontVariantCaps
        | PropertyId::Font
        | PropertyId::LineHeight
        | PropertyId::LetterSpacing
        | PropertyId::WordSpacing
        | PropertyId::Color
        | PropertyId::TextAlign
        | PropertyId::TextIndent
        | PropertyId::WhiteSpace
        | PropertyId::TextTransform
        | PropertyId::Direction
        | PropertyId::Visibility => Some(PropertyGroup::Text),

        // ── Background / visual (non-inherited) ──────────────────
        PropertyId::BackgroundColor
        | PropertyId::Background
        | PropertyId::BackgroundImage
        | PropertyId::BackgroundPosition
        | PropertyId::BackgroundSize
        | PropertyId::BackgroundRepeat
        | PropertyId::BorderTopColor
        | PropertyId::BorderRightColor
        | PropertyId::BorderBottomColor
        | PropertyId::BorderLeftColor
        | PropertyId::BorderColor
        | PropertyId::BorderTopStyle
        | PropertyId::BorderRightStyle
        | PropertyId::BorderBottomStyle
        | PropertyId::BorderLeftStyle
        | PropertyId::BorderStyle
        | PropertyId::BorderTopLeftRadius(..)
        | PropertyId::BorderTopRightRadius(..)
        | PropertyId::BorderBottomLeftRadius(..)
        | PropertyId::BorderBottomRightRadius(..)
        | PropertyId::BorderRadius(..)
        | PropertyId::BoxShadow(..)
        | PropertyId::Opacity
        | PropertyId::OutlineColor
        | PropertyId::OutlineStyle
        | PropertyId::OutlineWidth => Some(PropertyGroup::Background),

        // ── Box model (non-inherited) ────────────────────────────
        PropertyId::Width
        | PropertyId::Height
        | PropertyId::MinWidth
        | PropertyId::MinHeight
        | PropertyId::MaxWidth
        | PropertyId::MaxHeight
        | PropertyId::Margin
        | PropertyId::MarginTop
        | PropertyId::MarginRight
        | PropertyId::MarginBottom
        | PropertyId::MarginLeft
        | PropertyId::MarginBlock
        | PropertyId::MarginBlockStart
        | PropertyId::MarginBlockEnd
        | PropertyId::MarginInline
        | PropertyId::MarginInlineStart
        | PropertyId::MarginInlineEnd
        | PropertyId::Padding
        | PropertyId::PaddingTop
        | PropertyId::PaddingRight
        | PropertyId::PaddingBottom
        | PropertyId::PaddingLeft
        | PropertyId::PaddingBlock
        | PropertyId::PaddingBlockStart
        | PropertyId::PaddingBlockEnd
        | PropertyId::PaddingInline
        | PropertyId::PaddingInlineStart
        | PropertyId::PaddingInlineEnd
        | PropertyId::BorderTopWidth
        | PropertyId::BorderRightWidth
        | PropertyId::BorderBottomWidth
        | PropertyId::BorderLeftWidth
        | PropertyId::BorderWidth
        | PropertyId::BoxSizing(..) => Some(PropertyGroup::BoxModel),

        // ── Layout mode (non-inherited) ──────────────────────────
        PropertyId::Display
        | PropertyId::FlexDirection(..)
        | PropertyId::FlexWrap(..)
        | PropertyId::FlexFlow(..)
        | PropertyId::FlexGrow(..)
        | PropertyId::FlexShrink(..)
        | PropertyId::FlexBasis(..)
        | PropertyId::Flex(..)
        | PropertyId::JustifyContent(..)
        | PropertyId::AlignItems(..)
        | PropertyId::AlignSelf(..)
        | PropertyId::AlignContent(..)
        | PropertyId::Order(..)
        | PropertyId::GridTemplateColumns
        | PropertyId::GridTemplateRows
        | PropertyId::GridTemplateAreas
        | PropertyId::GridAutoColumns
        | PropertyId::GridAutoRows
        | PropertyId::GridAutoFlow
        | PropertyId::GridColumn
        | PropertyId::GridRow
        | PropertyId::GridColumnStart
        | PropertyId::GridColumnEnd
        | PropertyId::GridRowStart
        | PropertyId::GridRowEnd
        | PropertyId::Gap
        | PropertyId::RowGap
        | PropertyId::ColumnGap
        | PropertyId::Overflow
        | PropertyId::OverflowX
        | PropertyId::OverflowY => Some(PropertyGroup::Layout),

        // ── Position (non-inherited) ─────────────────────────────
        PropertyId::Position
        | PropertyId::Top
        | PropertyId::Right
        | PropertyId::Bottom
        | PropertyId::Left
        | PropertyId::InsetBlockStart
        | PropertyId::InsetBlockEnd
        | PropertyId::InsetInlineStart
        | PropertyId::InsetInlineEnd
        | PropertyId::Inset
        | PropertyId::InsetBlock
        | PropertyId::InsetInline
        | PropertyId::ZIndex => Some(PropertyGroup::Position),

        _ => None,
    }
}
