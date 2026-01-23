use crate::value::CssValue;

/// CSS properties that can be queried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Query)]
#[value_type(CssValue)]
pub enum CssProperty {
    // Box Model - Sizing (dimensional properties moved to dimensional.rs)
    #[query(get_css_property)]
    BoxSizing,

    // Border Style
    #[query(get_css_property)]
    BorderTopStyle,
    #[query(get_css_property)]
    BorderRightStyle,
    #[query(get_css_property)]
    BorderBottomStyle,
    #[query(get_css_property)]
    BorderLeftStyle,

    // Border Color
    #[query(get_css_property)]
    BorderTopColor,
    #[query(get_css_property)]
    BorderRightColor,
    #[query(get_css_property)]
    BorderBottomColor,
    #[query(get_css_property)]
    BorderLeftColor,

    // Border Radius
    #[query(get_css_property)]
    BorderTopLeftRadius,
    #[query(get_css_property)]
    BorderTopRightRadius,
    #[query(get_css_property)]
    BorderBottomRightRadius,
    #[query(get_css_property)]
    BorderBottomLeftRadius,

    // Display & Visibility
    #[query(get_css_property)]
    Display,
    #[query(get_css_property)]
    Visibility,
    #[query(get_css_property)]
    Opacity,

    // Positioning (Top/Right/Bottom/Left moved to dimensional.rs)
    #[query(get_css_property)]
    Position,
    #[query(get_css_property)]
    ZIndex,

    // Float & Clear
    #[query(get_css_property)]
    Float,
    #[query(get_css_property)]
    Clear,

    // Overflow
    #[query(get_css_property)]
    Overflow,
    #[query(get_css_property)]
    OverflowX,
    #[query(get_css_property)]
    OverflowY,

    // Flexbox Container (RowGap/ColumnGap/Gap moved to dimensional.rs)
    #[query(get_css_property)]
    FlexDirection,
    #[query(get_css_property)]
    FlexWrap,
    #[query(get_css_property)]
    JustifyContent,
    #[query(get_css_property)]
    AlignItems,
    #[query(get_css_property)]
    AlignContent,

    // Flexbox Item (FlexGrow/FlexShrink moved to dimensional.rs for i32 return type)
    #[query(get_css_property)]
    FlexBasis,
    #[query(get_css_property)]
    AlignSelf,
    #[query(get_css_property)]
    Order,

    // Grid Container
    #[query(get_css_property)]
    GridTemplateColumns,
    #[query(get_css_property)]
    GridTemplateRows,
    #[query(get_css_property)]
    GridTemplateAreas,
    #[query(get_css_property)]
    GridAutoColumns,
    #[query(get_css_property)]
    GridAutoRows,
    #[query(get_css_property)]
    GridAutoFlow,
    #[query(get_css_property)]
    JustifyItems,

    // Grid Item
    #[query(get_css_property)]
    GridColumnStart,
    #[query(get_css_property)]
    GridColumnEnd,
    #[query(get_css_property)]
    GridRowStart,
    #[query(get_css_property)]
    GridRowEnd,
    #[query(get_css_property)]
    GridColumn,
    #[query(get_css_property)]
    GridRow,
    #[query(get_css_property)]
    GridArea,
    #[query(get_css_property)]
    JustifySelf,

    // Typography - Font
    #[query(get_css_property)]
    FontFamily,
    #[query(get_css_property)]
    FontSize,
    #[query(get_css_property)]
    FontWeight,
    #[query(get_css_property)]
    FontStyle,
    #[query(get_css_property)]
    FontVariant,
    #[query(get_css_property)]
    FontStretch,
    #[query(get_css_property)]
    LineHeight,

    // Typography - Text
    #[query(get_css_property)]
    Color,
    #[query(get_css_property)]
    TextAlign,
    #[query(get_css_property)]
    TextDecoration,
    #[query(get_css_property)]
    TextDecorationLine,
    #[query(get_css_property)]
    TextDecorationColor,
    #[query(get_css_property)]
    TextDecorationStyle,
    #[query(get_css_property)]
    TextDecorationThickness,
    #[query(get_css_property)]
    TextTransform,
    #[query(get_css_property)]
    TextIndent,
    #[query(get_css_property)]
    TextOverflow,
    #[query(get_css_property)]
    TextShadow,

    // Typography - White Space
    #[query(get_css_property)]
    WhiteSpace,
    #[query(get_css_property)]
    WordBreak,
    #[query(get_css_property)]
    WordWrap,
    #[query(get_css_property)]
    OverflowWrap,
    #[query(get_css_property)]
    WordSpacing,
    #[query(get_css_property)]
    LetterSpacing,

    // Background
    #[query(get_css_property)]
    BackgroundColor,
    #[query(get_css_property)]
    BackgroundImage,
    #[query(get_css_property)]
    BackgroundPosition,
    #[query(get_css_property)]
    BackgroundSize,
    #[query(get_css_property)]
    BackgroundRepeat,
    #[query(get_css_property)]
    BackgroundAttachment,
    #[query(get_css_property)]
    BackgroundOrigin,
    #[query(get_css_property)]
    BackgroundClip,

    // List Style
    #[query(get_css_property)]
    ListStyleType,
    #[query(get_css_property)]
    ListStylePosition,
    #[query(get_css_property)]
    ListStyleImage,

    // Table
    #[query(get_css_property)]
    TableLayout,
    #[query(get_css_property)]
    BorderCollapse,
    #[query(get_css_property)]
    BorderSpacing,
    #[query(get_css_property)]
    CaptionSide,
    #[query(get_css_property)]
    EmptyCells,

    // Transform
    #[query(get_css_property)]
    Transform,
    #[query(get_css_property)]
    TransformOrigin,
    #[query(get_css_property)]
    TransformStyle,
    #[query(get_css_property)]
    Perspective,
    #[query(get_css_property)]
    PerspectiveOrigin,
    #[query(get_css_property)]
    BackfaceVisibility,

    // Transition
    #[query(get_css_property)]
    Transition,
    #[query(get_css_property)]
    TransitionProperty,
    #[query(get_css_property)]
    TransitionDuration,
    #[query(get_css_property)]
    TransitionTimingFunction,
    #[query(get_css_property)]
    TransitionDelay,

    // Animation
    #[query(get_css_property)]
    Animation,
    #[query(get_css_property)]
    AnimationName,
    #[query(get_css_property)]
    AnimationDuration,
    #[query(get_css_property)]
    AnimationTimingFunction,
    #[query(get_css_property)]
    AnimationDelay,
    #[query(get_css_property)]
    AnimationIterationCount,
    #[query(get_css_property)]
    AnimationDirection,
    #[query(get_css_property)]
    AnimationFillMode,
    #[query(get_css_property)]
    AnimationPlayState,

    // Filter & Effects
    #[query(get_css_property)]
    Filter,
    #[query(get_css_property)]
    BackdropFilter,
    #[query(get_css_property)]
    MixBlendMode,
    #[query(get_css_property)]
    BoxShadow,

    // Outline
    #[query(get_css_property)]
    OutlineWidth,
    #[query(get_css_property)]
    OutlineStyle,
    #[query(get_css_property)]
    OutlineColor,
    #[query(get_css_property)]
    OutlineOffset,

    // Cursor & Pointer
    #[query(get_css_property)]
    Cursor,
    #[query(get_css_property)]
    PointerEvents,
    #[query(get_css_property)]
    UserSelect,

    // Content & Quotes
    #[query(get_css_property)]
    Content,
    #[query(get_css_property)]
    Quotes,
    #[query(get_css_property)]
    CounterIncrement,
    #[query(get_css_property)]
    CounterReset,

    // Writing Modes
    #[query(get_css_property)]
    WritingMode,
    #[query(get_css_property)]
    Direction,
    #[query(get_css_property)]
    UnicodeBidi,

    // Columns
    #[query(get_css_property)]
    ColumnCount,
    #[query(get_css_property)]
    ColumnWidth,
    #[query(get_css_property)]
    ColumnRule,
    #[query(get_css_property)]
    ColumnRuleWidth,
    #[query(get_css_property)]
    ColumnRuleStyle,
    #[query(get_css_property)]
    ColumnRuleColor,
    #[query(get_css_property)]
    ColumnSpan,
    #[query(get_css_property)]
    ColumnFill,

    // Sizing (additional)
    #[query(get_css_property)]
    AspectRatio,
    #[query(get_css_property)]
    ObjectFit,
    #[query(get_css_property)]
    ObjectPosition,

    // Scroll
    #[query(get_css_property)]
    ScrollBehavior,
    #[query(get_css_property)]
    ScrollMargin,
    #[query(get_css_property)]
    ScrollPadding,
    #[query(get_css_property)]
    ScrollSnapType,
    #[query(get_css_property)]
    ScrollSnapAlign,

    // Other
    #[query(get_css_property)]
    Resize,
    #[query(get_css_property)]
    VerticalAlign,
    #[query(get_css_property)]
    Clip,
    #[query(get_css_property)]
    ClipPath,
    #[query(get_css_property)]
    Mask,
    #[query(get_css_property)]
    MaskImage,
    #[query(get_css_property)]
    MaskMode,
    #[query(get_css_property)]
    MaskRepeat,
    #[query(get_css_property)]
    MaskPosition,
    #[query(get_css_property)]
    MaskClip,
    #[query(get_css_property)]
    MaskOrigin,
    #[query(get_css_property)]
    MaskSize,
    #[query(get_css_property)]
    MaskComposite,
    #[query(get_css_property)]
    Isolation,
    #[query(get_css_property)]
    WillChange,
    #[query(get_css_property)]
    Contain,
    #[query(get_css_property)]
    ContentVisibility,
}

/// Query a CSS property value for a node.
///
/// This is a placeholder that will be specialized for each property variant
/// by the macro-generated code. Each variant gets its own query type that
/// internally knows which CSS property name to query.
///
/// NOTE: This function should not be called directly - use the generated
/// query types like DisplayQuery, PositionQuery, etc.
fn get_css_property(
    _db: &rewrite_core::Database,
    _node: rewrite_core::NodeId,
    _ctx: &mut rewrite_core::DependencyContext,
) -> CssValue {
    // This is a placeholder - the macro should generate specialized versions
    // for each property that know which property name to query
    todo!("Property queries need to be individually implemented with property names")
}
