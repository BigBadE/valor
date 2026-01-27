//! CSS Property definitions.
//!
//! This module defines all CSS properties split by their usage in the layout/rendering pipeline.
//! Properties with directional variants are unified with direction enums to minimize redundancy.

use crate::{Axis, Boundary, Corner, Edge, Position};

// ============================================================================
// Layout Properties (Keywords only - used by Query for branching)
// ============================================================================

/// CSS properties that control layout logic and structure.
/// These return Keywords and are used by Query code to branch on layout modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CssLayoutProperty {
    // Display & Flow
    Display,
    Position,
    Float,
    Clear,
    Overflow,
    OverflowAxis(Axis),

    // Box Model
    BoxSizing,

    // Flexbox Container
    FlexDirection,
    FlexWrap,
    JustifyContent,
    AlignItems,
    AlignContent,

    // Flexbox Item
    AlignSelf,

    // Grid Container
    GridAutoFlow,
    JustifyItems,

    // Grid Item
    JustifySelf,

    // Table
    TableLayout,
    BorderCollapse,
    CaptionSide,
    EmptyCells,

    // Writing Modes
    WritingMode,
    Direction,
    UnicodeBidi,

    // Visibility
    Visibility,

    // Other layout-affecting
    VerticalAlign,
}

// ============================================================================
// Value Properties (Return numeric values - used by Theorem for computation)
// ============================================================================

/// CSS properties that return concrete numeric values.
/// These are read by Theorem code to compute actual layout dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CssValueProperty {
    // Box Model - Edges
    Padding(Edge),
    Margin(Edge),
    BorderWidth(Edge),

    // Box Model - Sizing
    Size(Axis),    // Width, Height
    MinSize(Axis), // MinWidth, MinHeight
    MaxSize(Axis), // MaxWidth, MaxHeight
    AspectRatio,

    // Positioning
    Offset(Edge), // Top, Right, Bottom, Left
    ZIndex,

    // Flexbox/Grid Gaps
    Gap(Axis), // RowGap, ColumnGap

    // Flexbox Item
    FlexGrow,
    FlexShrink,
    FlexBasis,
    Order,

    // Grid Container
    GridTemplate(Axis), // GridTemplateColumns, GridTemplateRows
    GridTemplateAreas,
    GridAuto(Axis), // GridAutoColumns, GridAutoRows

    // Grid Item
    GridLine(Axis, Position), // GridColumnStart, GridColumnEnd, GridRowStart, GridRowEnd
    GridSpan(Axis),           // GridColumn, GridRow
    GridArea,

    // Typography - Font Sizing
    FontSize,
    LineHeight,

    // Typography - Spacing
    WordSpacing,
    LetterSpacing,
    TextIndent,

    // Columns
    ColumnCount,
    ColumnSize(Boundary),
    ColumnSpan,

    // Scroll
    ScrollMargin(Edge),
    ScrollPadding(Edge),

    // Outline
    OutlineWidth,
    OutlineOffset,
}

// ============================================================================
// Visual Properties (Used by rendering, not layout)
// ============================================================================

/// CSS properties that only affect visual rendering, not layout.
/// These are handled by the rendering system after layout is complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CssVisualProperty {
    // Colors
    Color,
    BackgroundColor,
    BorderColor(Edge),
    OutlineColor,

    // Border/Outline Styles
    BorderStyle(Edge),
    BorderRadius(Corner),
    OutlineStyle,

    // Background
    BackgroundImage,
    BackgroundPosition,
    BackgroundSize,
    BackgroundRepeat,
    BackgroundAttachment,
    BackgroundOrigin,
    BackgroundClip,

    // Typography - Font Properties
    FontFamily,
    FontWeight,
    FontStyle,
    FontVariant,
    FontStretch,

    // Typography - Text Styling
    TextAlign,
    TextDecoration,
    TextDecorationLine,
    TextDecorationColor,
    TextDecorationStyle,
    TextDecorationThickness,
    TextTransform,
    TextOverflow,
    TextShadow,

    // Typography - White Space
    WhiteSpace,
    WordBreak,
    WordWrap,
    OverflowWrap,

    // List Style
    ListStyleType,
    ListStylePosition,
    ListStyleImage,

    // Table Visual
    BorderSpacing,

    // Transform
    Transform,
    TransformOrigin,
    TransformStyle,
    Perspective,
    PerspectiveOrigin,
    BackfaceVisibility,

    // Transition
    Transition,
    TransitionProperty,
    TransitionDuration,
    TransitionTimingFunction,
    TransitionDelay,

    // Animation
    Animation,
    AnimationName,
    AnimationDuration,
    AnimationTimingFunction,
    AnimationDelay,
    AnimationIterationCount,
    AnimationDirection,
    AnimationFillMode,
    AnimationPlayState,

    // Filter & Effects
    Filter,
    BackdropFilter,
    MixBlendMode,
    BoxShadow,
    Opacity,

    // Cursor & Pointer
    Cursor,
    PointerEvents,
    UserSelect,

    // Content & Quotes
    Content,
    Quotes,
    CounterIncrement,
    CounterReset,

    // Columns Visual
    ColumnRule,
    ColumnRuleWidth,
    ColumnRuleStyle,
    ColumnRuleColor,
    ColumnFill,

    // Object
    ObjectFit,
    ObjectPosition,

    // Scroll Visual
    ScrollBehavior,
    ScrollSnapType,
    ScrollSnapAlign,

    // Other
    Resize,
    Clip,
    ClipPath,
    Mask,
    MaskImage,
    MaskMode,
    MaskRepeat,
    MaskPosition,
    MaskClip,
    MaskOrigin,
    MaskSize,
    MaskComposite,
    Isolation,
    WillChange,
    Contain,
    ContentVisibility,
}
